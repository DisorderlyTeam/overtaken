/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use engine::scene::Component;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PlayerComponent {
    detectable: bool,
    invisible: bool,
    enemies_paused: bool,
    invincible: bool,
    dead: bool,
}

impl Default for PlayerComponent {
    fn default() -> Self {
        Self {
            detectable: true,
            invisible: false,
            enemies_paused: false,
            invincible: false,
            dead: false,
        }
    }
}

impl PlayerComponent {
    pub fn set_detectable(&mut self, detectable: bool) {
        self.detectable = detectable;
    }

    pub fn is_detectable(&self) -> bool {
        self.detectable
    }

    pub fn toggle_invisibility(&mut self) {
        self.invisible = !self.invisible;
    }

    pub fn is_invisible(&self) -> bool {
        self.invisible
    }

    pub fn toggle_enemy_pause(&mut self) {
        self.enemies_paused = !self.enemies_paused;
    }

    pub fn are_enemies_paused(&self) -> bool {
        self.enemies_paused
    }

    pub fn toggle_invincibility(&mut self) {
        self.invincible = !self.invincible;
    }

    pub fn is_invincible(&self) -> bool {
        self.invincible
    }

    pub fn set_dead(&mut self, dead: bool) {
        self.dead = dead;
    }

    pub fn is_dead(&self) -> bool {
        self.dead
    }
}

impl Component for PlayerComponent {}
