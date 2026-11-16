/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::context::RenderCtx;
use crate::core::vec2d::Array2D;
use crate::render_world::{Camera2d, SceneRenderData, ShadowSegment, SpriteInstance, SpriteSpace};
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, LocalTransform, Material, Scene};
use glam::{Mat4, USizeVec2, UVec2, Vec2, Vec3};
use std::ops::{Index, IndexMut};

pub struct TilemapComponent {
    pub tileset: Tileset,
    pub map: Array2D<Option<UVec2>>,
    pub lighting_layer: Option<u8>,
    pub shadow_collision_mask: u32,
}

impl TilemapComponent {
    pub fn new(tileset: Tileset, map: Array2D<Option<UVec2>>) -> Self {
        Self {
            tileset,
            map,
            lighting_layer: None,
            shadow_collision_mask: u32::MAX,
        }
    }

    pub fn from_indices<const W: usize, const H: usize>(
        tileset: Tileset,
        indices: [[u32; W]; H],
    ) -> Self {
        let map = Array2D::new_init(W, H, |x, y| {
            let index = indices[y][x];
            let (tile_x, tile_y) = (index % tileset.tile_count.x, index / tileset.tile_count.x);
            Some(UVec2::new(tile_x, tile_y))
        });
        Self::new(tileset, map)
    }

    pub fn size(&self) -> UVec2 {
        USizeVec2::from(self.map.size()).as_uvec2()
    }

    pub fn width(&self) -> u32 {
        self.size().x
    }

    pub fn height(&self) -> u32 {
        self.size().y
    }

    pub fn get(&self, x: u32, y: u32) -> Option<UVec2> {
        *self.map.get(x as usize, y as usize)
    }

    pub fn set_lighting_layer(&mut self, layer: Option<u8>) {
        self.lighting_layer = layer;
    }

    pub fn set_shadow_collision_mask(&mut self, mask: u32) {
        self.shadow_collision_mask = mask;
    }

    /// Converts tile coordinates to object-space center position for a given tile size.
    pub fn tile_to_object_position_with_size(tile: UVec2, tile_size: Vec2) -> Vec3 {
        Vec3::new(
            tile.x as f32 * tile_size.x,
            tile.y as f32 * tile_size.y,
            0.0,
        )
    }

    /// Returns center position of the tile in object space.
    pub fn tile_to_object_position(&self, tile: UVec2) -> Vec3 {
        Self::tile_to_object_position_with_size(tile, self.tileset.tile_size)
    }

    /// Returns object-space AABB bounds (min, max) for the given tile.
    pub fn tile_to_object_bounds(&self, tile: UVec2) -> (Vec3, Vec3) {
        let center = self.tile_to_object_position(tile);
        let half = (self.tileset.tile_size * 0.5).extend(0.0);
        (center - half, center + half)
    }

    /// Finds which tile range is visible on the screen.
    ///
    /// Returns `None` if nothing is visible; otherwise, returns tuple of two coordinates of visible tiles at the edges.
    pub fn find_visible_tile_range(
        &self,
        object_to_world: Mat4,
        camera: Camera2d,
        render_resolution: UVec2,
    ) -> Option<(UVec2, UVec2)> {
        if self.width() == 0 || self.height() == 0 {
            return None;
        }

        let camera_to_clip = camera.camera_to_clip(render_resolution.as_vec2());
        let world_to_camera = camera.world_to_camera();

        let object_to_clip = camera_to_clip * world_to_camera * object_to_world;
        let clip_to_object = object_to_clip.inverse();

        // Get the corners of the screen in clip space
        let clip_corners = [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, 1.0),
        ];

        let object_corners = clip_corners.map(|clip_coords| {
            clip_to_object
                .transform_point3(clip_coords.extend(0.0))
                .truncate()
        });

        let object_min = object_corners
            .iter()
            .cloned()
            .reduce(Vec2::min)
            .expect("corners should not be empty");
        let object_max = object_corners
            .iter()
            .cloned()
            .reduce(Vec2::max)
            .expect("corners should not be empty");

        let half_tile = self.tileset.tile_size * 0.5;
        let tile_min = (object_min - half_tile) / self.tileset.tile_size;
        let tile_max = (object_max + half_tile) / self.tileset.tile_size;
        let tile_min = tile_min.ceil().as_uvec2();
        let tile_max = tile_max.ceil().as_uvec2();

        let min_bound = UVec2::ZERO;
        let max_bound = self.size() - UVec2::new(1, 1);

        // Check if anything is within the screen
        if tile_max.x < min_bound.x
            || tile_min.x > max_bound.x
            || tile_max.y < min_bound.y
            || tile_min.y > max_bound.y
        {
            return None;
        }

        let tile_min = tile_min.clamp(min_bound, max_bound);
        let tile_max = tile_max.clamp(min_bound, max_bound);

        debug_assert!(
            tile_min.x <= tile_max.x && tile_min.y <= tile_max.y,
            "tile bounds are out of order"
        );

        Some((tile_min, tile_max))
    }
}

impl Index<UVec2> for TilemapComponent {
    type Output = Option<UVec2>;

    fn index(&self, index: UVec2) -> &Self::Output {
        &self.map.get(index.x as usize, index.y as usize)
    }
}

impl IndexMut<UVec2> for TilemapComponent {
    fn index_mut(&mut self, index: UVec2) -> &mut Self::Output {
        self.map.get_mut(index.x as usize, index.y as usize)
    }
}

impl Component for TilemapComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::NEEDS_DRAW
    }

    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    ) {
        let Some(local) = scene.get_component::<LocalTransform>(entity) else {
            return;
        };
        let Some(global) = scene.get_component::<GlobalTransform>(entity) else {
            return;
        };

        let object_to_world = global.object_to_world;

        let (map_width, map_height) = self.size().into();
        let tile_size = self.tileset.tile_size;
        let tile_count = self.tileset.tile_count;
        let tile_uv_size = 1.0 / tile_count.as_vec2();

        let Some((tile_min, tile_max)) = self.find_visible_tile_range(
            object_to_world,
            render_data.camera,
            render_data.render_resolution,
        ) else {
            return;
        };

        for x in tile_min.x..=tile_max.x {
            for y in tile_min.y..=tile_max.y {
                let Some(tile_coord) = self.get(x, y) else {
                    continue;
                };

                let tile_to_world = object_to_world
                    * Mat4::from_translation(self.tile_to_object_position(UVec2::new(x, y)))
                    * Mat4::from_scale(tile_size.extend(1.0));

                let tile_coord = tile_coord.as_vec2();
                let tile_uv_min = tile_coord * tile_uv_size;
                let tile_uv_max = tile_uv_min + tile_uv_size;

                render_data.sprites.push(SpriteInstance {
                    material: self.tileset.material,
                    object_to_world: tile_to_world,
                    depth: local.position.z,
                    uv_min: tile_uv_min,
                    uv_max: tile_uv_max,
                    space: SpriteSpace::World,
                    lighting_layer: self.lighting_layer,
                });
            }
        }

        let Some(collisions) = self.tileset.collisions.as_ref() else {
            return;
        };
        let shadow_mask = self.shadow_collision_mask;
        let is_solid = |x: i32, y: i32| -> bool {
            if x < 0 || y < 0 || x >= map_width as i32 || y >= map_height as i32 {
                return false;
            }
            let Some(tile) = self.get(x as u32, y as u32) else {
                return false;
            };
            if tile.x >= tile_count.x || tile.y >= tile_count.y {
                return false;
            }
            let layer = *collisions.get(tile.x as usize, tile.y as usize);
            layer != 0 && (layer & shadow_mask) != 0
        };

        let shadow_layer = self.lighting_layer.unwrap_or(0);
        let mut push_segment = |a: Vec2, b: Vec2| {
            let aw = object_to_world.transform_point3(a.extend(0.0)).truncate();
            let bw = object_to_world.transform_point3(b.extend(0.0)).truncate();
            // We make the segment *ever so slightly* bigger to eliminate corner artifacts.
            let d = (bw - aw).normalize() * 0.1;
            render_data.push_shadow_segment(shadow_layer, ShadowSegment(aw - d, bw + d));
        };

        // Top and bottom edges, merged into horizontal runs.
        for y in 0..map_height as i32 {
            let mut top_run_start: Option<i32> = None;
            let mut bottom_run_start: Option<i32> = None;
            for x in 0..=map_width as i32 {
                let top_exposed = x < map_width as i32 && is_solid(x, y) && !is_solid(x, y - 1);
                let bottom_exposed = x < map_width as i32 && is_solid(x, y) && !is_solid(x, y + 1);

                if top_exposed {
                    top_run_start.get_or_insert(x);
                } else if let Some(x0) = top_run_start.take() {
                    let (start_min, _) =
                        self.tile_to_object_bounds(UVec2::new(x0 as u32, y as u32));
                    let (_, end_max) =
                        self.tile_to_object_bounds(UVec2::new((x - 1) as u32, y as u32));
                    let a = Vec2::new(start_min.x, start_min.y);
                    let b = Vec2::new(end_max.x, start_min.y);
                    push_segment(a, b);
                }

                if bottom_exposed {
                    bottom_run_start.get_or_insert(x);
                } else if let Some(x0) = bottom_run_start.take() {
                    let (start_min, start_max) =
                        self.tile_to_object_bounds(UVec2::new(x0 as u32, y as u32));
                    let (_, end_max) =
                        self.tile_to_object_bounds(UVec2::new((x - 1) as u32, y as u32));
                    let a = Vec2::new(start_min.x, start_max.y);
                    let b = Vec2::new(end_max.x, start_max.y);
                    push_segment(a, b);
                }
            }
        }

        // Left and right edges, merged into vertical runs.
        for x in 0..map_width as i32 {
            let mut left_run_start: Option<i32> = None;
            let mut right_run_start: Option<i32> = None;
            for y in 0..=map_height as i32 {
                let left_exposed = y < map_height as i32 && is_solid(x, y) && !is_solid(x - 1, y);
                let right_exposed = y < map_height as i32 && is_solid(x, y) && !is_solid(x + 1, y);

                if left_exposed {
                    left_run_start.get_or_insert(y);
                } else if let Some(y0) = left_run_start.take() {
                    let (start_min, _) =
                        self.tile_to_object_bounds(UVec2::new(x as u32, y0 as u32));
                    let (_, end_max) =
                        self.tile_to_object_bounds(UVec2::new(x as u32, (y - 1) as u32));
                    let a = Vec2::new(start_min.x, start_min.y);
                    let b = Vec2::new(start_min.x, end_max.y);
                    push_segment(a, b);
                }

                if right_exposed {
                    right_run_start.get_or_insert(y);
                } else if let Some(y0) = right_run_start.take() {
                    let (start_min, start_max) =
                        self.tile_to_object_bounds(UVec2::new(x as u32, y0 as u32));
                    let (_, end_max) =
                        self.tile_to_object_bounds(UVec2::new(x as u32, (y - 1) as u32));
                    let a = Vec2::new(start_max.x, start_min.y);
                    let b = Vec2::new(start_max.x, end_max.y);
                    push_segment(a, b);
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Tileset {
    pub material: Material,
    pub tile_size: Vec2,
    pub tile_count: UVec2,
    pub collisions: Option<Array2D<u32>>,
}
