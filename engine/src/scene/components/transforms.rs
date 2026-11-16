/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::render_world::SpriteSpace;
use crate::scene::components::Component;
use glam::{Mat4, Vec2, Vec3};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct LocalTransform {
    pub position: Vec3,
    pub scale: Vec2,
    pub rotation: f32,
    pub ignore_parent_transform: bool,
    pub screen_space: bool,
}

impl Component for LocalTransform {}

impl Default for LocalTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
            ignore_parent_transform: false,
            screen_space: false,
        }
    }
}

impl LocalTransform {
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            position: Vec3::new(x, y, z),
            ..Self::default()
        }
    }

    pub fn from_xyz_screenspace(x: f32, y: f32, z: f32) -> Self {
        Self {
            position: Vec3::new(x, y, z),
            screen_space: true,
            ..Self::default()
        }
    }

    pub fn as_mat4(&self) -> Mat4 {
        Mat4::from_translation(self.position)
            * Mat4::from_rotation_z(self.rotation)
            * Mat4::from_scale(Vec3::new(self.scale.x, self.scale.y, 1.0))
    }

    pub fn space(&self) -> SpriteSpace {
        if self.screen_space {
            SpriteSpace::Screen
        } else {
            SpriteSpace::World
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct GlobalTransform {
    pub object_to_world: Mat4,
}

impl GlobalTransform {
    pub fn new(object_to_world: Mat4) -> Self {
        Self { object_to_world }
    }
}

impl Component for GlobalTransform {}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self::new(Mat4::IDENTITY)
    }
}
