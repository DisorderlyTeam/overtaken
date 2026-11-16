/*
 * Copyright (c) 2026. DisorderlyTeam
 */

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VisibilityComponent {
    visible: bool,
    effective_visible: bool,
}

impl Default for VisibilityComponent {
    fn default() -> Self {
        Self {
            visible: true,
            effective_visible: true,
        }
    }
}

impl VisibilityComponent {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            effective_visible: visible,
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn is_effectively_visible(&self) -> bool {
        self.effective_visible
    }

    pub(crate) fn set_effective_visible(&mut self, visible: bool) {
        self.effective_visible = visible;
    }
}

impl super::Component for VisibilityComponent {}
