// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use std::{collections::HashMap, path::Path};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const MAX_REQUESTS_PER_DAY: u8 = 20;

#[derive(Default, Serialize, Deserialize)]
pub struct Memory(HashMap<String, Person>);

impl Memory {
    pub fn load<P: AsRef<Path>>(file: P) -> anyhow::Result<Self> {
        let path = file.as_ref();

        let file = match std::fs::read_to_string(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)
                            .context("Failed to create memory directory")?;
                    }
                }
                std::fs::write(path, "{}")
                    .context("Failed to initialize memory file")?;
                String::from("{}")
            }
            Err(e) => {
                return Err(e).context("Failed to read the memory file");
            }
        };
        if file.trim().is_empty() {
            return Ok(Self::default());
        }

        let memory: Self =
            serde_json::from_str(&file).context("Failed to parse memory file")?;
        Ok(memory)
    }

    pub fn save<P: AsRef<Path>>(&self, file: P) -> anyhow::Result<()> {
        let json = serde_json::to_string(self).context("Failed to serialize the memory")?;
        std::fs::write(file, json).context("Failed to save the memory")?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Person> {
        self.0.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Person> {
        self.0.get_mut(id)
    }

    pub fn insert(&mut self, id: &str) -> Person {
        let _ = self.0.insert(id.to_owned(), Person::default());
        Person::default()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Person {
    request_count_start: DateTime<Utc>,
    request_count: u8,
    pub affinity: Affinity,
    pub talk_count: u32,
    pub memo: String,
    #[serde(default)]
    pub impression: String,
    #[serde(default)]
    pub affinity_logs: Vec<AffinityLog>,
}

impl Person {
    pub fn is_rate_limited(&self) -> bool {
        self.request_count > MAX_REQUESTS_PER_DAY
            && (Utc::now() - self.request_count_start).num_days() < 1
    }

    pub fn update_talk_count(&mut self) {
        let now = Utc::now();
        if (now - self.request_count_start).num_days() >= 1 {
            self.request_count_start = now;
            self.request_count = 1;
        } else {
            self.request_count += 1;
        }

        self.talk_count += 1;
    }

    pub fn push_affinity_log(&mut self, change: &str, reason: String, before: i8, after: i8) {
        const MAX_AFFINITY_LOGS: usize = 100;

        self.affinity_logs.push(AffinityLog {
            timestamp: Utc::now(),
            change: change.to_owned(),
            before,
            after,
            reason,
        });

        if self.affinity_logs.len() > MAX_AFFINITY_LOGS {
            let overflow = self.affinity_logs.len() - MAX_AFFINITY_LOGS;
            self.affinity_logs.drain(0..overflow);
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AffinityLog {
    pub timestamp: DateTime<Utc>,
    pub change: String,
    pub before: i8,
    pub after: i8,
    pub reason: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Affinity {
    value: i8,
}

impl Affinity {
    pub const fn tick_positive(&mut self) {
        self.value = self.value.saturating_add(1);
    }

    pub const fn tick_negative(&mut self) {
        self.value = self.value.saturating_sub(1);
    }

    pub const fn affinity(&self) -> i8 {
        match self.value {
            ..=-122 => -5,
            -121..=-41 => -4,
            -40..=-14 => -3,
            -13..=-5 => -2,
            -4..=-2 => -1,
            -1..=1 => 0,
            2..=4 => 1,
            5..=13 => 2,
            14..=40 => 3,
            41..=121 => 4,
            122.. => 5,
        }
    }
}
