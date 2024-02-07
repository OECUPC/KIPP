//! タスクの追加を行う実装
//! エラーがserenityと別なので注意する.
//! エラーはすべてserenityのものへ統一化

// Copyright © 2024 donabe8898. All rights reserved.

use futures_util::TryFutureExt;
use poise::reply::ReplyHandle;
use poise::serenity_prelude::*;
use poise::*;

use serde::{de::IntoDeserializer, Deserialize, Serialize};
use std::time::Duration; // タイムアウト処理用
use std::{any::Any, thread::sleep};
use tokio;
use tokio::time::timeout;
use tokio_postgres::{
    tls::{NoTlsStream, TlsConnect},
    Client, Connection, Error, NoTls, Row, Socket,
};

use super::*;
type Context<'a> = poise::Context<'a, super::Data, serenity::Error>;

#[poise::command(slash_command)]
pub async fn test(ctx: Context<'_>) -> Result<(), serenity::Error> {
    //! DB test command
    /* 返答用string */
    let mut response = String::new();

    /* コマンドを実行したチャンネルのIDを取得 */
    let channel_id: String = ctx.channel_id().to_string();

    /*
    DBへの接続を試行

    tokio_postgres::Errorをserenity::Errorで返すことでエラー処理の簡略化と統一化を図る
    */
    let (client, conn) = match db_conn().await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Connected error: {}", e);
            return Err(serenity::Error::Other("Database connection error".into()));
        }
    };

    /* 接続タスク実行 */
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("connection error: {}", e);
            eprintln!("コネクションエラー: {}", e);
        }
    });

    /* DBテーブル取得 */
    let rows = match client.query("select * from testdb", &[]).await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("query error: {}", e);
            return Err(serenity::Error::Other("Query error".into()));
        }
    };

    /* 表示とdiscord返信 */
    for row in rows {
        let id: i32 = row.get(0);
        let name: &str = row.get(1);
        response += &format!("id: {}, name: {}\n", id, name);
    }
    let _ = ctx.say(response).await;
    Ok(())
}

// TODO: タスクの追加
/// タスクを1件追加します
#[poise::command(slash_command)]
pub async fn addtask(
    ctx: Context<'_>,
    #[description = "タスク名"] task_name: String,
    #[description = "担当者"] member: serenity::Member,
) -> Result<(), serenity::Error> {
    /* コマンドを実行したチャンネルのIDを取得 */
    let channel_id = ctx.channel_id();

    /*
    DBへの接続を試行

    tokio_postgres::Errorをserenity::Errorで返すことでエラー処理の簡略化と統一化を図る
    */
    let (client, conn) = match db_conn().await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Connected error: {}", e);
            return Err(serenity::Error::Other("Database connection error".into()));
        }
    };

    /* 接続タスク実行 */
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("connection err: {}", e);
        }
    });

    /*
    タスク登録
    テーブルが無ければ作成

    NOTE: ステータス
    - 進行中 = 1
    - 終了 = 0
    */

    let tsk_name: String = task_name;
    let member_id: UserId = member.user.id;

    // レコード作成用クエリ文
    let insert = format!(
        "insert into \"{}\" (id, task_name, member, status) values (uuid_generate_v4(), \'{}\', \'{}\', 1);",
        channel_id, tsk_name, member_id
    );

    // テーブル作成用クエリ文
    let create = format!("create table \"{}\" (id uuid DEFAULT uuid_generate_v4(), task_name text NOT NULL, member text NOT NULL, status smallint DEFAULT 1);",channel_id);

    // クエリ送信
    let _res = match client.query(&insert, &[]).await {
        Ok(_result) => {}
        Err(_e) => {
            let _e_res = match client.query(&create, &[]).await {
                Ok(_result) => {
                    let _ = client.query(&insert, &[]).await;
                }
                Err(_e) => {
                    return Err(serenity::Error::Other("タスクの登録に失敗しました".into()));
                }
            };
        }
    };

    /* 完了メッセージ */

    let _ = ctx.reply("タスクを登録しました").await;
    Ok(())
}

// TODO: タスクの完了

/// ボタンテスト→ステータス変更コマンド
#[poise::command(slash_command)]
pub async fn change(
    ctx: Context<'_>,
    #[description = "タスクID"] task_id: String,
) -> Result<(), serenity::Error> {
    /* コマンドを実行したチャンネルのIDを取得 */
    let channel_id = ctx.channel_id();
    // DB処理

    // DB処理

    // ----------Yesボタン----------
    let mut btn_run = CreateButton::new("running")
        .emoji('\u{1f697}')
        .label("進行中")
        .style(ButtonStyle::Success);

    // ----------Noボタン----------
    let mut btn_done = CreateButton::new("done")
        .emoji('\u{2615}')
        .label("完了")
        .style(ButtonStyle::Secondary);

    // ----------アクションにボタンを追加----------
    let mut buttons = CreateActionRow::Buttons(vec![btn_run, btn_done]);

    // let rep = CreateReply::default()
    //     .content("")
    //     .content("ステータスをどれに変更しますか？")
    //     .components(vec![buttons]);

    let rep2 = CreateMessage::default().components(vec![buttons]);

    let _ = ctx.say("ステータスをどれに変更しますか？").await;

    let h = channel_id.send_message(ctx, rep2).await;

    let handle = match h {
        Ok(result) => result,
        Err(_) => panic!("送信失敗"),
    };

    let mi = match handle
        .await_component_interaction(&ctx)
        .timeout(Duration::from_secs(10))
        .await
    {
        Some(interaction) => interaction,
        None => {
            let _ = handle.delete(ctx).await;

            return Err(serenity::Error::Other("タイムアウトしました。".into()));
        }
    };

    let _ = handle.delete(ctx).await;

    let id: &str = &mi.data.custom_id;
    let _ = match id {
        "running" => {
            channel_id
                .send_message(
                    ctx,
                    CreateMessage::default().content("進行中に設定しました"),
                )
                .await
        }
        "done" => {
            channel_id
                .send_message(
                    ctx,
                    CreateMessage::default().content("完了済みに設定しました"),
                )
                .await
        }
        _ => {
            panic!("エラー");
        }
    };

    Ok(())
}

/// データベースへの接続処理
pub async fn db_conn() -> Result<(Client, Connection<Socket, NoTlsStream>), Error> {
    let (client, conn) = tokio_postgres::Config::new()
        .user("postgres")
        .password("password")
        .host("localhost")
        .port(5432)
        .dbname("postgres")
        .connect(tokio_postgres::NoTls)
        .await?;

    Ok((client, conn))
}
