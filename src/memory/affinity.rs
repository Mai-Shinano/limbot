// SPDX-FileCopyrightText: 2026 SyoBoN <syobon@syobon.net>
//
// SPDX-License-Identifier: UPL-1.0

use serde::{Deserialize, Serialize};

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
