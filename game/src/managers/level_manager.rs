/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use std::fs::read_to_string;
use std::ops::Deref;

pub struct LevelManager {
    level_list: Vec<String>,
}

impl Default for LevelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelManager {
    fn new() -> Self {
        let level_list_json =
            read_to_string("assets/data/levels.json").expect("couldn't read levels data");
        let level_list =
            serde_json::from_str(&level_list_json).expect("couldn't parse levels data");
        LevelManager { level_list }
    }

    pub fn try_get_next_level(&self, current: &str) -> Option<&str> {
        let current_index = self.level_list.iter().position(|it| it == current)?;
        Some(self.level_list.get(current_index + 1)?.deref())
    }

    pub fn first_level(&self) -> &str {
        &self.level_list[0]
    }

    pub fn levels(&self) -> impl Iterator<Item = &str> {
        self.level_list.iter().map(|it| it.as_str())
    }

    pub fn level_of_index(&self, index: u32) -> Option<&str> {
        self.level_list.get(index as usize).map(|it| it.as_str())
    }

    pub fn count(&self) -> usize {
        self.level_list.len()
    }
}
