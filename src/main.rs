// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use std::sync::Arc;

use anyhow::Context;
use env_logger::Env;
use llmbot::{AICore, ContextMessage};
use megalodon::{
    Megalodon,
    default::NO_REDIRECT,
    entities::{Status, StatusVisibility, notification::NotificationType},
    megalodon::{AppInputOptions, PostStatusInputOptions},
    streaming::Message,
};

mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("llmbot=info"));

    let config = config::Config::load("config.toml")?;
    let llm_token = config.llm_token()?;

    if config.sns_token.is_none() {
        let token = authorize(config).await?;
        println!("token generated: {token}");
        return Ok(());
    }

    let mastodon = megalodon::generator(
        config.sns.clone(),
        config.sns_url.clone(),
        config.sns_token.clone(),
        None,
    )
    .context("Failed to build a client")?;
    let _ = mastodon
        .verify_account_credentials()
        .await
        .context("Failed to verify credentials")?;
    let mastodon: Arc<dyn Megalodon + Send + Sync> = Arc::from(mastodon);

    let ai = AICore::new(
        &config.memory_file,
        &config.openai_url,
        &llm_token,
        &config.openai_model,
        &config.master_acct,
        &config.instruction,
    );
    let ai = Arc::new(ai);

    let streaming = mastodon.user_streaming().await;
    streaming
        .listen(Box::new(|message| {
            let mastodon = Arc::clone(&mastodon);
            let ai = Arc::clone(&ai);
            Box::pin({
                async move {
                    let Message::Notification(notification) = message else {
                        return;
                    };
                    if notification.r#type == NotificationType::Mention {
                        let Some(status) = notification.status else {
                            return;
                        };

                        tokio::spawn(async move {
                            let mastodon = Arc::clone(&mastodon);
                            let ai = Arc::clone(&ai);
                            process(&*mastodon, &ai, status).await;
                        });
                    }
                }
            })
        }))
        .await;

    Ok(())
}

async fn process(mastodon: &(dyn Megalodon + Send + Sync), ai: &AICore, status: Status) {
    let context: Vec<ContextMessage> = mastodon
        .get_status_context(status.id.clone(), None)
        .await
        .map(|ctx| {
            ctx.json
                .ancestors
                .into_iter()
                .map(|status| ContextMessage {
                    name: status.account.display_name,
                    content: status
                        .plain_content
                        .unwrap_or_else(|| nanohtml2text::html2text(&status.content))
                        .trim()
                        .to_owned(),
                })
                .collect()
        })
        .inspect_err(|e| log::error!("{e:?}"))
        // 失敗した場合コンテキストなしで続ける
        .unwrap_or_default();

    let content = status
        .plain_content
        .unwrap_or_else(|| nanohtml2text::html2text(&status.content))
        .trim()
        .to_owned();

    let visibility = if status.visibility == StatusVisibility::Public {
        StatusVisibility::Unlisted
    } else {
        status.visibility
    };

    match ai
        .generate(
            &status.account.acct,
            &status.account.display_name,
            &content,
            context,
        )
        .await
    {
        Ok(response) => {
            let _ = mastodon
                .post_status(
                    response,
                    Some(&PostStatusInputOptions {
                        in_reply_to_id: Some(status.id),
                        visibility: Some(visibility),
                        ..Default::default()
                    }),
                )
                .await
                .inspect_err(|e| log::error!("{e:?}"));
        }
        Err(e) => {
            let _ = mastodon
                .post_status(
                    format!("エラーだよ。\n\n{e:?}"),
                    Some(&PostStatusInputOptions {
                        in_reply_to_id: Some(status.id),
                        visibility: Some(visibility),
                        ..Default::default()
                    }),
                )
                .await
                .inspect_err(|e| log::error!("{e:?}"));
        }
    }
}

async fn authorize(config: config::Config) -> anyhow::Result<String> {
    let client = megalodon::generator(config.sns, config.sns_url, None, None)
        .context("Failed to build a client")?;

    let options = AppInputOptions {
        scopes: Some(vec![
            String::from("read"),
            String::from("write"),
            // String::from("follow"), // いまのところ使わない
        ]),
        ..Default::default()
    };

    let app_data = client
        .register_app(String::from("syoboneko"), &options)
        .await
        .context("Failed to register an app")?;

    println!(
        "Authorization URL is generated: {}\n\nEnter authorization code from website: ",
        app_data.url.unwrap()
    );
    let mut buf = String::new();
    std::io::stdin()
        .read_line(&mut buf)
        .context("Failed to read from stdin")?;

    let token_data = client
        .fetch_access_token(
            app_data.client_id,
            app_data.client_secret,
            buf.trim().to_owned(),
            NO_REDIRECT.to_owned(),
        )
        .await
        .context("Failed to get an access token")?;

    Ok(token_data.access_token)
}
