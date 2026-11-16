/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::core::color::Color;
use crate::scene::{Material, TextBufferHandle, TextLayout, TextStyle};
use glam::{Mat4, UVec2, Vec2, Vec3, Vec3Swizzles, Vec4};
use std::f32::consts::TAU;

pub const DEBUG_DRAW_LAYER_GENERIC: u32 = 1 << 0;
pub const DEBUG_DRAW_LAYER_COLLISIONS: u32 = 1 << 1;
pub const DEBUG_DRAW_LAYER_LIGHTS: u32 = 1 << 2;

#[derive(Default)]
pub struct RenderWorld {
    pub frame_index: u64,
    pub render_resolution: UVec2,
    pub scene_passes: Vec<SceneRenderData>,
}

impl RenderWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_frame(&mut self, frame_index: u64, render_resolution: UVec2) {
        self.frame_index = frame_index;
        self.render_resolution = render_resolution;
        self.scene_passes.clear();
    }
}

pub struct SceneRenderData {
    pub camera: Camera2d,
    pub render_resolution: UVec2,
    pub sprites: Vec<SpriteInstance>,
    pub debug_drawables: Vec<DebugDrawableInstance>,
    pub texts: Vec<TextRenderInstance>,
    pub lights_by_layer: Vec<Vec<LightSource>>,
    pub shadow_segments_by_layer: Vec<Vec<ShadowSegment>>,
    pub shown_debug_layers: u32,
    pub lighting_layer_count: u8,
}

impl SceneRenderData {
    pub fn new(
        camera: Camera2d,
        render_resolution: UVec2,
        shown_debug_layers: u32,
        lighting_layer_count: u8,
    ) -> Self {
        // We try some reasonable defaults to avoid reallocating Vecs too often.
        Self {
            camera,
            render_resolution,
            sprites: Vec::with_capacity(128),
            debug_drawables: if shown_debug_layers != 0 {
                Vec::with_capacity(512)
            } else {
                // Hopefully if debug draw is disabled, no debug drawables should be added to the render world.
                // They wouldn't be rendered anyway.
                Vec::new()
            },
            texts: Vec::with_capacity(32),
            lights_by_layer: (0..lighting_layer_count)
                .map(|_| Vec::with_capacity(16))
                .collect(),
            shadow_segments_by_layer: (0..lighting_layer_count)
                .map(|_| Vec::with_capacity(512))
                .collect(),
            shown_debug_layers,
            lighting_layer_count,
        }
    }

    pub fn push_shadow_segment(&mut self, layer: u8, segment: ShadowSegment) {
        if let Some(layer_segments) = self.shadow_segments_by_layer.get_mut(layer as usize) {
            layer_segments.push(segment);
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Camera2d {
    pub center: Vec2,
    pub zoom: f32,
    pub rotation: f32,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

impl Camera2d {
    pub fn world_to_camera(&self) -> Mat4 {
        let translate = Mat4::from_translation(-Vec3::new(self.center.x, self.center.y, 0.0));
        let rotate = Mat4::from_rotation_z(-self.rotation);
        let scale = Mat4::from_scale(Vec3::new(self.zoom, self.zoom, 1.0));

        scale * rotate * translate
    }

    pub fn camera_to_clip(&self, render_resolution: Vec2) -> Mat4 {
        Mat4::orthographic_rh(
            -render_resolution.x / 2.0,
            render_resolution.x / 2.0,
            render_resolution.y / 2.0,
            -render_resolution.y / 2.0,
            -1000.0,
            1000.0,
        )
    }
}

pub struct SpriteInstance {
    pub material: Material,
    pub object_to_world: Mat4,
    pub depth: f32,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    pub space: SpriteSpace,
    pub lighting_layer: Option<u8>,
}

pub struct TextRenderInstance {
    pub text: String,
    pub style: TextStyle,
    pub layout: TextLayout,
    pub buffer: TextBufferHandle,
    pub object_to_world: Mat4,
    pub depth: f32,
    pub space: SpriteSpace,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SpriteSpace {
    World,
    Screen,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DebugShape {
    Rect {
        width: f32,
        height: f32,
    },
    Circle {
        radius: f32,
        segments: u32,
    },
    Arc {
        radius: f32,
        direction: Vec2,
        angle: f32,
        segments: u32,
    },
    Star {
        radius: f32,
    },
    Polygon {
        points: Vec<Vec2>,
    },
}

impl DebugShape {
    pub fn outline_vertices(&self) -> Vec<Vec2> {
        match self {
            DebugShape::Rect { width, height } => {
                let half_w = *width * 0.5;
                let half_h = *height * 0.5;
                let p0 = Vec2::new(-half_w, -half_h);
                let p1 = Vec2::new(half_w, -half_h);
                let p2 = Vec2::new(half_w, half_h);
                let p3 = Vec2::new(-half_w, half_h);
                vec![p0, p1, p1, p2, p2, p3, p3, p0]
            }
            DebugShape::Circle { radius, segments } => {
                let segment_count = (*segments).max(3);
                let mut vertices = Vec::with_capacity((segment_count as usize) * 2);
                for i in 0..segment_count {
                    let a0 = TAU * (i as f32) / (segment_count as f32);
                    let a1 = TAU * ((i + 1) as f32) / (segment_count as f32);
                    vertices.push(Vec2::new(a0.cos() * *radius, a0.sin() * *radius));
                    vertices.push(Vec2::new(a1.cos() * *radius, a1.sin() * *radius));
                }
                vertices
            }
            DebugShape::Arc {
                radius,
                direction,
                angle,
                segments,
            } => {
                let segment_count = (*segments).max(1);
                let dir = if direction.length_squared() > 1e-10 {
                    direction.normalize()
                } else {
                    Vec2::X
                };
                let perp = Vec2::new(-dir.y, dir.x);
                let half_angle = *angle * 0.5;
                let point_at =
                    |theta: f32| -> Vec2 { (dir * theta.cos() + perp * theta.sin()) * *radius };

                let mut vertices = Vec::with_capacity((segment_count as usize) * 2 + 4);
                for i in 0..segment_count {
                    let t0 = -half_angle + *angle * (i as f32) / (segment_count as f32);
                    let t1 = -half_angle + *angle * ((i + 1) as f32) / (segment_count as f32);
                    vertices.push(point_at(t0));
                    vertices.push(point_at(t1));
                }

                let start = point_at(-half_angle);
                let end = point_at(half_angle);
                vertices.push(Vec2::ZERO);
                vertices.push(start);
                vertices.push(Vec2::ZERO);
                vertices.push(end);
                vertices
            }
            DebugShape::Star { radius } => {
                let r = *radius;
                vec![
                    Vec2::new(-r, 0.0),
                    Vec2::new(r, 0.0),
                    Vec2::new(0.0, -r),
                    Vec2::new(0.0, r),
                    Vec2::new(-r, -r),
                    Vec2::new(r, r),
                    Vec2::new(-r, r),
                    Vec2::new(r, -r),
                ]
            }
            DebugShape::Polygon { points } => {
                if points.len() < 2 {
                    return Vec::new();
                }

                let mut vertices = Vec::with_capacity(points.len() * 2);
                for i in 0..points.len() {
                    let curr = points[i];
                    let next = points[(i + 1) % points.len()];
                    vertices.push(curr);
                    vertices.push(next);
                }
                vertices
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugDrawable {
    pub shape: DebugShape,
    pub color: Vec4,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugDrawableInstance {
    pub object_to_world: Mat4,
    pub depth: f32,
    pub color: Vec4,
    /// Uses line list topology, so each pair of vertices forms a separate line segment.
    ///
    /// E.g. vertices A, B, C, D draw two lines: A-B and C-D.
    pub vertices: Vec<Vec2>,
    pub space: SpriteSpace,
    pub layer: u32,
}

/// Represents a light source.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct LightSource {
    pub position: Vec2,
    pub radius: f32,
    pub color: Color,
    pub intensity: f32,
    pub direction: Vec2,
    /// Full spotlight cone angle in radians.
    ///
    /// `TAU` means full circle (point light), `PI` means half circle.
    pub angle: f32,
    pub lighting_layer: u8,
}

impl LightSource {
    pub fn new(
        position: Vec2,
        radius: f32,
        color: Color,
        intensity: f32,
        direction: Vec2,
        angle: f32,
        lighting_layer: u8,
    ) -> Self {
        Self {
            position,
            radius,
            color,
            intensity,
            direction,
            angle,
            lighting_layer,
        }
    }
}

/// Represents a segment that casts shadows.
///
/// Points are in world space.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ShadowSegment(pub Vec2, pub Vec2);

impl ShadowSegment {
    /// Extrudes the shadow segment by a given distance from a reference point.
    ///
    /// `from` should be a clip-space point. World-space `self` will be projected into clip space
    /// using the given [`world_to_camera`] matrix.
    pub fn extrude(self, world_to_clip: Mat4, from: Vec2, distance: f32) -> [Vec2; 4] {
        let p0 = world_to_clip.project_point3(self.0.extend(0.0)).xy();
        let p1 = world_to_clip.project_point3(self.1.extend(0.0)).xy();
        let seg = p1 - p0;
        let seg_dir = if seg.length_squared() > 1e-10 {
            seg.normalize()
        } else {
            Vec2::X
        };

        let extrude_projected_point = |point: Vec2| -> Vec2 {
            let ray = point - from;
            if ray.length_squared() <= 1e-10 {
                return point;
            }
            let ray_dir = ray.normalize();
            let sin_theta = (ray_dir.x * seg_dir.y - ray_dir.y * seg_dir.x).abs();
            let angle_scale = 1.0 / sin_theta.max(1e-3);
            point + ray_dir * (distance * angle_scale)
        };

        [
            p0,
            p1,
            extrude_projected_point(p0),
            extrude_projected_point(p1),
        ]
    }
}
