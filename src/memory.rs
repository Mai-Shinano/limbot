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
        let file = std::fs::read_to_string(file).context("Failed to read the memory file")?;
        let memory: Self = serde_json::from_str(&file).context("Failed to perse memory file")?;
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
            ..=-62 => -5,
            -61..=-30 => -4,
            -29..=-14 => -3,
            -13..=-6 => -2,
            -5..=-2 => -1,
            -1..=1 => 0,
            2..=5 => 1,
            6..=13 => 2,
            14..=29 => 3,
            30..=61 => 4,
            62.. => 5,
        }
    }
}
