/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use engine::scene::Component;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum OvertakingRequirement {
    Any,
    Required,
    Forbidden,
}

#[derive(Debug, Clone)]
pub struct InteractionRequirements {
    pub overtaking: OvertakingRequirement,
    pub required_tags: Vec<String>,
}

impl Default for InteractionRequirements {
    fn default() -> Self {
        Self {
            overtaking: OvertakingRequirement::Any,
            required_tags: Vec::new(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InteractionAction {
    BeginOvertakeEnemy,
    BreakObstacle,
    VentAccess,
}

#[derive(Debug, Clone)]
pub struct InteractableComponent {
    requirements: InteractionRequirements,
    action: InteractionAction,
}

impl InteractableComponent {
    pub fn new(requirements: InteractionRequirements, action: InteractionAction) -> Self {
        Self {
            requirements,
            action,
        }
    }

    pub fn action(&self) -> InteractionAction {
        self.action
    }

    pub fn requirements_met(&self, is_overtaking: bool, active_tags: &[String]) -> bool {
        let overtaking_ok = match self.requirements.overtaking {
            OvertakingRequirement::Any => true,
            OvertakingRequirement::Required => is_overtaking,
            OvertakingRequirement::Forbidden => !is_overtaking,
        };
        overtaking_ok
            && self
                .requirements
                .required_tags
                .iter()
                .all(|required| active_tags.iter().any(|tag| tag == required))
    }
}

impl Component for InteractableComponent {}
