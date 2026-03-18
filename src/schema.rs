// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

// OpenAI Related

#[derive(Deserialize)]
pub struct OpenAIResponse {
    pub choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
pub struct OpenAIChoice {
    pub message: OpenAIResponseMessage,
}

#[derive(Deserialize)]
pub struct OpenAIResponseMessage {
    pub content: String,
}

// Input to an LLM

#[derive(Serialize)]
pub struct Input {
    pub context: Vec<ContextMessage>,
    pub person: Person,
    pub datetime: DateTime<Local>,
    pub content: String,
}

#[derive(Serialize)]
pub struct ContextMessage {
    pub name: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub is_master: bool,
    pub affinity: i8,
    pub talk_count: u32,
    pub memo: String,
}

// Output from an LLM

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
pub struct Output {
    pub reasoning: String,
    pub affinity_change: AffinityChange,
    pub affinity_reason: String,
    pub memo_update: MemoUpdate,
    pub response: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffinityChange {
    Up,
    Down,
    Unchanged,
}

impl AffinityChange {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Unchanged => "unchanged",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoUpdateMode {
    Overwrite,
    NoChanges,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoUpdate {
    pub mode: MemoUpdateMode,
    pub content: Option<String>,
}
