// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

// use serde::{Deserialize, Serialize};
use serde::Deserialize;

// #[derive(Serialize, Deserialize)]
// #[serde(tag = "role", rename_all = "snake_case")]
// pub enum Message {
//     System {
//         content: String,
//     },
//     User {
//         content: String,
//     },
//     Assistant {
//         content: String,
//         tool_cals: Option<Vec<ToolCall>>,
//     },
//     Tool {
//         content: String,
//         tool_call_id: String,
//     },
// }

// #[derive(Serialize, Deserialize)]
// pub struct ToolCall {
//     pub id: String,
//     #[serde(rename = "type")]
//     call_type: ToolCallType,
//     pub function: Function,
// }

// #[derive(Serialize, Deserialize)]
// pub enum ToolCallType {
//     Function,
// }

// #[derive(Serialize, Deserialize)]
// pub struct Function {
//     pub name: String,
//     pub arguments: Option<String>,
// }

#[derive(Deserialize)]
pub struct Response {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize)]
pub struct Choice {
    // pub finish_reason: FinishReason,
    pub message: ResponseMessage,
}

// #[derive(Deserialize)]
// #[serde(rename_all = "snake_case")]
// pub enum FinishReason {
//     Stop,
//     ToolCalls,
// }

#[derive(Deserialize)]
pub struct ResponseMessage {
    pub content: String,
    // pub tool_calls: Option<Vec<ToolCall>>,
}
