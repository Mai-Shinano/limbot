// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{Context, bail};
use chrono::Timelike;
use env_logger::Env;
use llmbot::{AICore, ContextMessage};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("llmbot=info"));

    let config = config::Config::load("config.toml")?;
    if !config.sns.eq_ignore_ascii_case("misskey") {
        bail!("Only sns = \"Misskey\" is supported in this build.");
    }

    let sns_token = config
        .sns_token
        .clone()
        .context("Misskey requires sns_token in config.toml")?;
    let llm_token = config.llm_token()?;

    let misskey = Arc::new(MisskeyClient::new(config.sns_url.clone(), sns_token));
    let me = misskey
        .verify_credentials()
        .await
        .context("Failed to verify Misskey credentials")?;

    let ai = Arc::new(AICore::new(
        &config.memory_file,
        &config.openai_url,
        &llm_token,
        &config.openai_model,
        &config.master_acct,
        &config.instruction,
    ));

    if let Some(interval_sec) = config.random_post_interval_sec.filter(|v| *v > 0) {
        let misskey = Arc::clone(&misskey);
        let ai = Arc::clone(&ai);
        let my_id = me.id.clone();
        let visibility = config
            .random_post_visibility
            .clone()
            .unwrap_or_else(|| String::from("home"));
        let quiet_hours = match (
            config.random_post_quiet_start_hour,
            config.random_post_quiet_end_hour,
        ) {
            (Some(start), Some(end)) if start < 24 && end < 24 => Some((start, end)),
            (Some(_), Some(_)) => {
                log::warn!(
                    "random_post_quiet_start_hour/end_hour must be in 0..=23. quiet hours disabled"
                );
                None
            }
            _ => None,
        };
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval_sec)).await;

                if let Some((start, end)) = quiet_hours {
                    let hour = chrono::Local::now().hour() as u8;
                    if is_quiet_hour(hour, start, end) {
                        continue;
                    }
                }

                let home_timeline_samples = misskey
                    .fetch_home_timeline(20)
                    .await
                    .map(|notes| {
                        notes
                            .into_iter()
                            .filter(|n| n.user.id != my_id)
                            .map(|n| n.content())
                            .collect::<Vec<_>>()
                    })
                    .inspect_err(|e| log::warn!("Failed to fetch home timeline: {e:?}"))
                    .unwrap_or_default();

                match ai.generate_random_post(home_timeline_samples).await {
                    Ok(text) => {
                        if text.trim().is_empty() {
                            continue;
                        }
                        let _ = misskey
                            .post_note(&text, &visibility)
                            .await
                            .inspect_err(|e| log::error!("{e:?}"));
                    }
                    Err(e) => {
                        log::error!("Failed to generate random post: {e:?}");
                    }
                }
            }
        });
    }

    if let Some(interval_sec) = config.proactive_reply_interval_sec.filter(|v| *v > 0) {
        let misskey = Arc::clone(&misskey);
        let ai = Arc::clone(&ai);
        let my_id = me.id.clone();
        let visibility = config
            .proactive_reply_visibility
            .clone()
            .unwrap_or_else(|| String::from("home"));
        let probability_percent = config
            .proactive_reply_probability_percent
            .unwrap_or(35)
            .min(100);
        let max_per_hour = config.proactive_reply_max_per_hour.unwrap_or(4);
        let replied_note_ids = Arc::new(Mutex::new(HashSet::<String>::new()));

        tokio::spawn(async move {
            let mut current_hour = chrono::Local::now().hour();
            let mut hourly_count: u16 = 0;

            loop {
                tokio::time::sleep(Duration::from_secs(interval_sec)).await;

                let now = chrono::Local::now();
                if now.hour() != current_hour {
                    current_hour = now.hour();
                    hourly_count = 0;
                }
                if hourly_count >= max_per_hour {
                    continue;
                }
                if !should_act_by_probability(probability_percent) {
                    continue;
                }

                let timeline = match misskey.fetch_home_timeline(30).await {
                    Ok(notes) => notes,
                    Err(e) => {
                        log::warn!("Failed to fetch home timeline for proactive reply: {e:?}");
                        continue;
                    }
                };

                let mut replied = replied_note_ids.lock().await;
                let target = timeline.into_iter().find(|note| {
                    note.user.id != my_id
                        && note.user.is_followed.unwrap_or(false)
                        && !note.content().trim().is_empty()
                        && !replied.contains(&note.id)
                });

                let Some(target) = target else {
                    continue;
                };
                let _ = replied.insert(target.id.clone());
                drop(replied);

                let context = misskey
                    .fetch_conversation(&target.id)
                    .await
                    .map(|notes| {
                        notes
                            .into_iter()
                            .filter(|ancestor| ancestor.id != target.id)
                            .map(|ancestor| ContextMessage {
                                name: ancestor.user.display_name(),
                                content: ancestor.content(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .inspect_err(|e| log::warn!("Failed to fetch proactive context: {e:?}"))
                    .unwrap_or_default();

                let account_id = target.user.acct();
                let display_name = target.user.display_name();
                let content = target.content();

                let reply = match ai
                    .generate(&account_id, &display_name, &content, context)
                    .await
                {
                    Ok(reply) => reply,
                    Err(e) => {
                        log::error!("Failed to generate proactive reply: {e:?}");
                        continue;
                    }
                };

                if reply.trim().is_empty() {
                    continue;
                }

                if misskey
                    .post_reply(&reply, &target.id, &visibility)
                    .await
                    .inspect_err(|e| log::error!("{e:?}"))
                    .is_ok()
                {
                    hourly_count = hourly_count.saturating_add(1);
                }
            }
        });
    }

    let processed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let mut since_id: Option<String> = None;

    // Ignore notifications that existed before this process starts.
    match misskey.fetch_notifications(None).await {
        Ok(initial) => {
            since_id = initial.first().map(|n| n.id.clone());
        }
        Err(e) => {
            log::warn!("Failed to initialize notification cursor: {e:?}");
        }
    }

    loop {
        match misskey.fetch_notifications(since_id.as_deref()).await {
            Ok(notifications) => {
                for notification in notifications.into_iter().rev() {
                    since_id = Some(notification.id.clone());

                    let Some(note) = notification.note else {
                        continue;
                    };

                    let mut processed = processed.lock().await;
                    if processed.contains(&note.id) {
                        continue;
                    }
                    let _ = processed.insert(note.id.clone());
                    drop(processed);

                    if note.user.id == me.id {
                        continue;
                    }

                    let misskey = Arc::clone(&misskey);
                    let ai = Arc::clone(&ai);
                    tokio::spawn(async move {
                        process(&misskey, &ai, note).await;
                    });
                }
            }
            Err(e) => {
                log::error!("{e:?}");
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn process(misskey: &MisskeyClient, ai: &AICore, note: Note) {
    let context = misskey
        .fetch_conversation(&note.id)
        .await
        .map(|notes| {
            notes
                .into_iter()
                .filter(|ancestor| ancestor.id != note.id)
                .map(|ancestor| ContextMessage {
                    name: ancestor.user.display_name(),
                    content: ancestor.content(),
                })
                .collect::<Vec<_>>()
        })
        .inspect_err(|e| log::error!("{e:?}"))
        .unwrap_or_default();

    let account_id = note.user.acct();
    let display_name = note.user.display_name();
    let content = note.content();

    let response = match ai
        .generate(&account_id, &display_name, &content, context)
        .await
    {
        Ok(response) => response,
        Err(e) => format!("エラーだよ。\n\n{e:?}"),
    };

    let visibility = normalize_visibility(note.visibility.as_deref());
    let _ = misskey
        .post_reply(&response, &note.id, visibility)
        .await
        .inspect_err(|e| log::error!("{e:?}"));
}

fn normalize_visibility(input: Option<&str>) -> &str {
    match input.unwrap_or("home") {
        "public" => "home",
        "home" => "home",
        "followers" => "followers",
        "specified" => "specified",
        _ => "home",
    }
}

fn is_quiet_hour(hour: u8, start: u8, end: u8) -> bool {
    if start == end {
        return false;
    }

    if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}

fn should_act_by_probability(percent: u8) -> bool {
    if percent == 0 {
        return false;
    }
    if percent >= 100 {
        return true;
    }

    let value = chrono::Local::now()
        .timestamp_nanos_opt()
        .unwrap_or_default()
        .unsigned_abs()
        % 100;
    value < u64::from(percent)
}

struct MisskeyClient {
    client: Client,
    base_url: String,
    token: String,
}

impl MisskeyClient {
    fn new(base_url: String, token: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            token,
        }
    }

    async fn verify_credentials(&self) -> anyhow::Result<MisskeyUser> {
        let body = IBody {
            i: self.token.as_str(),
        };

        self.post_json("/api/i", &body).await
    }

    async fn fetch_notifications(
        &self,
        since_id: Option<&str>,
    ) -> anyhow::Result<Vec<Notification>> {
        let body = NotificationsRequest {
            i: self.token.as_str(),
            // Reply notifications include non-mention threaded responses.
            include_types: vec!["mention", "reply"],
            limit: 30,
            since_id,
        };

        self.post_json("/api/i/notifications", &body).await
    }

    async fn fetch_conversation(&self, note_id: &str) -> anyhow::Result<Vec<Note>> {
        let body = ConversationRequest {
            i: self.token.as_str(),
            note_id,
            limit: 20,
        };

        self.post_json("/api/notes/conversation", &body).await
    }

    async fn fetch_home_timeline(&self, limit: u8) -> anyhow::Result<Vec<Note>> {
        let body = HomeTimelineRequest {
            i: self.token.as_str(),
            limit,
        };

        self.post_json("/api/notes/timeline", &body).await
    }

    async fn post_reply(&self, text: &str, reply_id: &str, visibility: &str) -> anyhow::Result<()> {
        let body = CreateNoteRequest {
            i: self.token.as_str(),
            text,
            reply_id: Some(reply_id),
            visibility,
        };

        let _value: serde_json::Value = self.post_json("/api/notes/create", &body).await?;
        Ok(())
    }

    async fn post_note(&self, text: &str, visibility: &str) -> anyhow::Result<()> {
        let body = CreateNoteRequest {
            i: self.token.as_str(),
            text,
            reply_id: None,
            visibility,
        };

        let _value: serde_json::Value = self.post_json("/api/notes/create", &body).await?;
        Ok(())
    }

    async fn post_json<TReq, TResp>(&self, path: &str, body: &TReq) -> anyhow::Result<TResp>
    where
        TReq: Serialize + ?Sized,
        TResp: for<'de> Deserialize<'de>,
    {
        let response = self
            .client
            .post(format!("{}{}", self.base_url.trim_end_matches('/'), path))
            .json(body)
            .send()
            .await
            .context("Failed to call Misskey API")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("Misskey API error {status}: {text}");
        }

        response
            .json::<TResp>()
            .await
            .context("Failed to parse Misskey API response")
    }
}

#[derive(Serialize)]
struct IBody<'a> {
    i: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationsRequest<'a> {
    i: &'a str,
    include_types: Vec<&'a str>,
    limit: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    since_id: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversationRequest<'a> {
    i: &'a str,
    note_id: &'a str,
    limit: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HomeTimelineRequest<'a> {
    i: &'a str,
    limit: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateNoteRequest<'a> {
    i: &'a str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_id: Option<&'a str>,
    visibility: &'a str,
}

#[derive(Deserialize)]
struct Notification {
    id: String,
    note: Option<Note>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Note {
    id: String,
    text: Option<String>,
    cw: Option<String>,
    visibility: Option<String>,
    user: MisskeyUser,
}

impl Note {
    fn content(&self) -> String {
        match (&self.cw, &self.text) {
            (Some(cw), Some(text)) if !cw.is_empty() => format!("{cw}\n{text}"),
            (Some(cw), None) if !cw.is_empty() => cw.clone(),
            (_, Some(text)) => text.clone(),
            _ => String::new(),
        }
    }
}

#[derive(Clone, Deserialize)]
struct MisskeyUser {
    id: String,
    username: String,
    host: Option<String>,
    name: Option<String>,
    #[serde(rename = "isFollowed")]
    is_followed: Option<bool>,
}

impl MisskeyUser {
    fn acct(&self) -> String {
        match self.host.as_deref() {
            Some(host) if !host.is_empty() => format!("{}@{host}", self.username),
            _ => self.username.clone(),
        }
    }

    fn display_name(&self) -> String {
        self.name
            .as_ref()
            .filter(|name| !name.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| self.username.clone())
    }
}
