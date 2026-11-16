/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::context::RenderCtx;
use crate::render_world::{SceneRenderData, SpriteInstance};
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, LocalTransform, Material, Scene};
use compact_str::CompactString;
use glam::{Mat4, Vec2};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

const DEFAULT_FPS: f32 = 6.0;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct AnimationCell {
    pub row: u32,
    pub column: u32,
}

impl<'de> Deserialize<'de> for AnimationCell {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AnimationCellVisitor;

        impl<'de> Visitor<'de> for AnimationCellVisitor {
            type Value = AnimationCell;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an animation cell as [x, y] or { row, column }")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let column = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let row = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                Ok(AnimationCell { row, column })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut row = None;
                let mut column = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "row" => row = Some(map.next_value()?),
                        "column" => column = Some(map.next_value()?),
                        _ => {
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                Ok(AnimationCell {
                    row: row.ok_or_else(|| serde::de::Error::missing_field("row"))?,
                    column: column.ok_or_else(|| serde::de::Error::missing_field("column"))?,
                })
            }
        }

        deserializer.deserialize_any(AnimationCellVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnimationClipDef {
    Frames {
        frames: Vec<AnimationCell>,
        #[serde(default = "default_fps")]
        fps: f32,
        #[serde(default)]
        looping: bool,
    },
    MoveDependent {
        idle_frame: AnimationCell,
        move_frames: Vec<AnimationCell>,
        #[serde(default = "default_fps")]
        fps: f32,
        #[serde(default)]
        looping: bool,
    },
}

impl AnimationClipDef {
    fn duration_s(&self) -> Option<f32> {
        match self {
            AnimationClipDef::Frames {
                frames,
                fps,
                looping,
            } => {
                if *looping || *fps <= f32::EPSILON || frames.len() <= 1 {
                    None
                } else {
                    Some(frames.len() as f32 / *fps)
                }
            }
            AnimationClipDef::MoveDependent {
                move_frames,
                fps,
                looping,
                ..
            } => {
                if *looping || *fps <= f32::EPSILON || move_frames.len() <= 1 {
                    None
                } else {
                    Some(move_frames.len() as f32 / *fps)
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AnimatedSpriteDefinition {
    pub frame_size: [u32; 2],
    pub default_state: String,
    #[serde(default)]
    pub flip_when_facing_left: bool,
    pub states: HashMap<String, AnimationClipDef>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Facing {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimatedSpriteComponent {
    pub material: Material,
    pub size: Vec2,
    definition: AnimatedSpriteDefinition,
    atlas_size: Vec2,
    state: CompactString,
    elapsed_s: f32,
    moving: bool,
    facing: Facing,
    lighting_layer: Option<u8>,
}

impl AnimatedSpriteComponent {
    pub fn new(
        material: Material,
        size: Vec2,
        atlas_size: Vec2,
        definition: AnimatedSpriteDefinition,
    ) -> Self {
        let state = CompactString::from(definition.default_state.as_str());
        Self {
            material,
            size,
            definition,
            atlas_size,
            state,
            elapsed_s: 0.0,
            moving: false,
            facing: Facing::Right,
            lighting_layer: None,
        }
    }

    pub fn with_lighting_layer(mut self, layer: u8) -> Self {
        self.lighting_layer = Some(layer);
        self
    }

    pub fn set_lighting_layer(&mut self, layer: Option<u8>) {
        self.lighting_layer = layer;
    }

    pub fn set_state(&mut self, state: impl Into<CompactString>) {
        let state = state.into();
        if self.state != state {
            self.state = state;
            self.elapsed_s = 0.0;
        }
    }

    pub fn state(&self) -> &str {
        self.state.as_str()
    }

    pub fn is_finished(&self) -> bool {
        self.clip()
            .and_then(AnimationClipDef::duration_s)
            .is_some_and(|duration| self.elapsed_s >= duration)
    }

    pub fn set_moving(&mut self, moving: bool) {
        self.moving = moving;
    }

    pub fn set_facing(&mut self, facing: Facing) {
        self.facing = facing;
    }

    pub fn set_facing_from_direction(&mut self, direction: Vec2) {
        if direction.x < 0.0 {
            self.facing = Facing::Left;
        } else if direction.x > 0.0 {
            self.facing = Facing::Right;
        }
    }

    pub fn advance(&mut self, dt: Duration) {
        self.elapsed_s += dt.as_secs_f32();
    }

    fn frame_size(&self) -> Vec2 {
        Vec2::new(
            self.definition.frame_size[0] as f32,
            self.definition.frame_size[1] as f32,
        )
    }

    fn clip(&self) -> Option<&AnimationClipDef> {
        self.definition.states.get(self.state.as_str()).or_else(|| {
            self.definition
                .states
                .get(self.definition.default_state.as_str())
        })
    }

    fn active_frame(&self) -> Option<AnimationCell> {
        let clip = self.clip()?;
        match clip {
            AnimationClipDef::Frames {
                frames,
                fps,
                looping,
            } => {
                if frames.is_empty() {
                    return None;
                }
                if frames.len() == 1 || *fps <= f32::EPSILON {
                    return Some(frames[0]);
                }
                let frame_index = (self.elapsed_s * *fps).floor() as usize;
                if *looping {
                    Some(frames[frame_index % frames.len()])
                } else {
                    Some(frames[frame_index.min(frames.len() - 1)])
                }
            }
            AnimationClipDef::MoveDependent {
                idle_frame,
                move_frames,
                fps,
                looping,
            } => {
                if !self.moving || move_frames.is_empty() || *fps <= f32::EPSILON {
                    return Some(*idle_frame);
                }
                let frame_index = (self.elapsed_s * *fps).floor() as usize;
                if *looping {
                    Some(move_frames[frame_index % move_frames.len()])
                } else {
                    Some(move_frames[frame_index.min(move_frames.len() - 1)])
                }
            }
        }
    }

    fn frame_uvs(&self, frame: AnimationCell) -> Option<(Vec2, Vec2)> {
        if self.atlas_size.x <= f32::EPSILON || self.atlas_size.y <= f32::EPSILON {
            return None;
        }

        let frame_size = self.frame_size();
        let min_px = Vec2::new(
            frame.column as f32 * frame_size.x,
            frame.row as f32 * frame_size.y,
        );
        let max_px = min_px + frame_size;
        let mut uv_min = min_px / self.atlas_size;
        let mut uv_max = max_px / self.atlas_size;

        if self.definition.flip_when_facing_left && self.facing == Facing::Left {
            std::mem::swap(&mut uv_min.x, &mut uv_max.x);
        }

        Some((uv_min, uv_max))
    }
}

impl Component for AnimatedSpriteComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::NEEDS_DRAW
    }

    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        _ctx: &mut RenderCtx<'_>,
    ) {
        let Some(local) = scene.get_component::<LocalTransform>(entity) else {
            return;
        };
        let Some(global) = scene.get_component::<GlobalTransform>(entity) else {
            return;
        };
        let Some(frame) = self.active_frame() else {
            return;
        };
        let Some((uv_min, uv_max)) = self.frame_uvs(frame) else {
            return;
        };

        let object_to_world = global.object_to_world * Mat4::from_scale(self.size.extend(1.0));
        render_data.sprites.push(SpriteInstance {
            material: self.material,
            object_to_world,
            depth: local.position.z,
            uv_min,
            uv_max,
            space: local.space(),
            lighting_layer: self.lighting_layer,
        });
    }
}

fn default_fps() -> f32 {
    DEFAULT_FPS
}
