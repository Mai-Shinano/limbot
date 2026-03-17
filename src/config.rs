// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub sns: String,
    pub sns_url: String,
    pub sns_token: Option<String>,
    pub openai_url: String,
    pub gemini_api_key: Option<String>,
    pub openai_token: Option<String>,
    pub model_token: Option<String>,
    pub openai_model: String,
    pub memory_file: PathBuf,
    pub master_acct: String,
    pub instruction: String,
}

impl Config {
    pub fn load<P: AsRef<Path>>(file: P) -> anyhow::Result<Self> {
        let file = std::fs::read_to_string(file).context("Failed to read the config file")?;
        let config: Self = toml::from_str(&file).context("Failed to parse the config")?;
        Ok(config)
    }

    pub fn llm_token(&self) -> anyhow::Result<String> {
        if let Some(token) = self
            .gemini_api_key
            .as_deref()
            .or(self.model_token.as_deref())
            .or(self.openai_token.as_deref())
        {
            return Ok(token.to_owned());
        }

        if let Ok(token) = std::env::var("GEMINI_API_KEY") {
            if !token.trim().is_empty() {
                return Ok(token);
            }
        }

        if let Ok(token) = std::env::var("MODEL_TOKEN") {
            if !token.trim().is_empty() {
                return Ok(token);
            }
        }

        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            if !token.trim().is_empty() {
                return Ok(token);
            }
        }

        if is_local_llm_endpoint(&self.openai_url) {
            return Ok(String::new());
        }

        bail!(
            "LLM token is not set. Set gemini_api_key/model_token/openai_token in config.toml, or GEMINI_API_KEY/MODEL_TOKEN/GITHUB_TOKEN in environment. For local LLM, use localhost/127.0.0.1 URL to allow no token."
        )
    }
}

fn is_local_llm_endpoint(url: &str) -> bool {
    let url = url.to_ascii_lowercase();
    url.contains("localhost") || url.contains("127.0.0.1") || url.contains("0.0.0.0")
}
