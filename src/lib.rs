// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use chrono::{DateTime, Local};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;

mod memory;
mod openai;

const SYSTEM_PROMPT_TEMPLATE: &str = r#"# 入力

入力は、以下のスキーマに従って与えられます。

```json
{"type":"object","properties":{"context":{"type":"array","description":"ここまでの会話内容","items":{"type":"object","properties":{"name":{"type":"string","description":"発言者のdisplay_name"},"content":{"type":"string","description":"発言内容"}}}},"person":{"type":"object","description":"会話相手に関する情報","properties":{"id":{"type":"string","description":"会話相手のid"},"name":{"type":"string","description":"会話相手のdisplay_name"},"affinity":{"type":"integer","description":"会話相手への好感度","minimum":-5,"maximum":5},"talk_count":{"type":"integer","description":"過去にこの相手と会話した回数"},"memo":{"type":"string","description":"会話相手に関するメモ"}}},"datetime":{"type":"string","description":"現在時刻"},"content":{"type":"string","description":"現在の会話内容"}}}
```

# 出力

別途与えられたスキーマに従い、思考内容、好感度の変動、メモの更新、応答内容を出力してください。
メモには、その人についての情報や、その人に対する印象を残すようにしてください。

応答内容は、以下に示すキャラクター設定に従い作成してください。また、好感度(-5から5)に合わせて態度を変化させるようにしてください。

## キャラクター設定"#;

#[derive(Serialize)]
pub struct ContextMessage {
    pub name: String,
    pub content: String,
}

#[derive(Serialize)]
struct Person {
    id: String,
    name: String,
    affinity: i8,
    talk_count: u32,
    memo: String,
}

#[derive(Serialize)]
struct Content {
    context: Vec<ContextMessage>,
    person: Person,
    datetime: DateTime<Local>,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AffinityChange {
    Up,
    Down,
    Unchanged,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MemoUpdateMode {
    Overwrite,
    NoChanges,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemoUpdate {
    mode: MemoUpdateMode,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Response {
    // reasoning: String,
    affinity_change: AffinityChange,
    memo_update: MemoUpdate,
    response: String,
}

pub struct App {
    client: Client,
    base_url: String,
    token: String,
    model: String,
    instruction: String,
    memory_file: PathBuf,
    memory: RwLock<memory::Memory>,
}

impl App {
    pub fn new<P: AsRef<Path>>(
        file: P,
        base_url: &str,
        token: &str,
        model: &str,
        instruction: &str,
    ) -> Self {
        let client = Client::new();
        let memory = memory::Memory::load(&file)
            .inspect_err(|e| log::warn!("{e:?}"))
            .unwrap_or_default();

        Self {
            client,
            memory_file: file.as_ref().to_owned(),
            memory: RwLock::new(memory),
            base_url: base_url.to_owned(),
            token: token.to_owned(),
            model: model.to_owned(),
            instruction: instruction.to_owned(),
        }
    }

    async fn request(&self, message: String) -> anyhow::Result<Response> {
        let request = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": format!("{SYSTEM_PROMPT_TEMPLATE}\n\n{}", self.instruction),
                },
                {
                    "role": "user",
                    "content": message,
                },
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "strict": true,
                    "schema": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["reasoning", "affinity_change", "memo_update", "response"],
                        "properties": {
                            "reasoning": {
                                "type": "string",
                                "description": "リクエストに対する思考過程",
                            },
                            "affinity_change": {
                                "type": "string",
                                "enum": ["up", "down", "unchanged"],
                                "description": "好感度の内部値を変化させるかどうか",
                            },
                            "memo_update": {
                                "type": "object",
                                "description": "メモの更新内容",
                                "additionalProperties": false,
                                "required": ["mode"],
                                "properties": {
                                    "mode": {
                                        "type": "string",
                                        "enum": ["overwrite", "no_changes"],
                                        "description": "メモの更新方法",
                                    },
                                    "content": {
                                        "type": "string",
                                        "description": "更新後の内容 (no_changesの場合省略可)",
                                    }
                                },
                            },
                            "response": {
                                "type": "string",
                                "description": "返信内容",
                            },
                        },
                    },
                },
            },
        });

        log::debug!("{request:?}");

        let provider_response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await
            .context("Failed to get response from the provider")?;
        let model_response: openai::Response = provider_response
            .json()
            .await
            .context("Failed to parse a response from the provider")?;

        let model_response_content = &model_response
            .choices
            .first()
            .context("Model did not respond")?
            .message
            .content;

        log::debug!("{model_response_content}");

        let response = serde_json::from_str(model_response_content)
            .context("Failed to parse the response from the model")?;

        Ok(response)
    }

    pub async fn generate(
        &self,
        account_id: &str,
        display_name: &str,
        content: &str,
        context: Vec<ContextMessage>,
    ) -> anyhow::Result<String> {
        let memory = self.memory.read().await;
        let person = if let Some(person) = memory.get(account_id) {
            let person = person.clone();
            drop(memory);
            person
        } else {
            drop(memory);
            let mut memory = self.memory.write().await;
            let person = memory.insert(account_id);
            drop(memory);
            person
        };

        if person.is_rate_limited() {
            bail!("レートリミットです。しばらく待ってから再度お試しください。");
        }

        let person = Person {
            id: account_id.to_owned(),
            name: display_name.to_owned(),
            affinity: person.affinity.affinity(),
            talk_count: person.talk_count,
            memo: person.memo.clone(),
        };
        let message_content = Content {
            context,
            person,
            datetime: Local::now(),
            content: content.to_owned(),
        };

        let message =
            serde_json::to_string(&message_content).context("Failed to serialize the context")?;

        let response = self.request(message).await?;
        log::debug!("{response:?}");

        let mut memory = self.memory.write().await;
        let person = memory.get_mut(account_id).unwrap();
        match response.affinity_change {
            AffinityChange::Up => person.affinity.tick_positive(),
            AffinityChange::Down => person.affinity.tick_negative(),
            AffinityChange::Unchanged => {}
        }
        match response.memo_update.mode {
            MemoUpdateMode::Overwrite => {
                person.memo = response.memo_update.content.unwrap_or_default();
            }
            MemoUpdateMode::NoChanges => {}
        }
        person.update_talk_count();

        // 保存に失敗しても続行
        let _ = memory
            .save(&self.memory_file)
            .inspect_err(|e| log::error!("{e:?}"));
        drop(memory);

        Ok(response.response)
    }
}
