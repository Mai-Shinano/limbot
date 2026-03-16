// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use std::path::{Path, PathBuf};

use anyhow::Context;
use megalodon::SNS;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub sns: SNS,
    pub sns_url: String,
    pub sns_token: Option<String>,
    pub openai_url: String,
    pub openai_token: String,
    pub openai_model: String,
    pub memory_file: PathBuf,
    pub instruction: String,
}

impl Config {
    pub fn load<P: AsRef<Path>>(file: P) -> anyhow::Result<Self> {
        let file = std::fs::read_to_string(file).context("Failed to read the config file")?;
        let config: Self = toml::from_str(&file).context("Failed to parse the config")?;
        Ok(config)
    }
}
