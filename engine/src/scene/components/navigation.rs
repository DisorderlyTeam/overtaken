/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::context::RenderCtx;
use crate::core::color::Color;
use crate::render_world::{DebugDrawableInstance, SceneRenderData, SpriteSpace};
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, Scene, TilemapComponent};
use glam::{Mat4, UVec2, Vec2, vec2};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

pub struct NavigationComponent {
    mesh: NavigationMesh,
    debug_path_triangles: Vec<usize>,
    debug_path_points: Vec<Vec2>,
}

pub struct NavigationMesh {
    /// Walkable navmesh triangles in world space.
    pub triangles: Vec<[Vec2; 3]>,
    /// Precomputed triangle centers used by A* heuristic and costs.
    triangle_centers: Vec<Vec2>,
    /// Precomputed triangle adjacency graph (shared-edge neighbors).
    adjacency: Vec<Vec<usize>>,
    /// Spatial hash from world-grid cell to candidate triangle indices.
    spatial_bins: HashMap<(i32, i32), Vec<usize>>,
    spatial_cell_size: f32,
}

#[derive(Debug, Clone)]
pub struct NavigationPath {
    pub waypoints: Vec<Vec2>,
    pub triangle_indices: Vec<usize>,
}

pub const NAVIGATION_MESH_AGENT_RADIUS: f32 = 5.0;

impl NavigationComponent {
    pub fn find_path(&self, start: Vec2, end: Vec2) -> Option<NavigationPath> {
        self.mesh.find_path(start, end)
    }

    pub fn set_debug_path_triangles(&mut self, triangle_indices: Vec<usize>) {
        self.debug_path_triangles = triangle_indices;
    }

    pub fn set_debug_path_points(&mut self, points: Vec<Vec2>) {
        self.debug_path_points = points;
    }

    pub fn from_tilemap(transform: &GlobalTransform, tilemap: &TilemapComponent) -> Self {
        Self::from_tilemap_with_block_mask(transform, tilemap, u32::MAX)
    }

    pub fn from_tilemap_with_block_mask(
        transform: &GlobalTransform,
        tilemap: &TilemapComponent,
        blocked_layers_mask: u32,
    ) -> Self {
        const R: f32 = NAVIGATION_MESH_AGENT_RADIUS;
        const EPS: f32 = 0.0001;

        let Some(collisions) = tilemap.tileset.collisions.as_ref() else {
            return Self {
                mesh: NavigationMesh::new(Vec::new()),
                debug_path_triangles: Vec::new(),
                debug_path_points: Vec::new(),
            };
        };
        let mut base_quads =
            Vec::with_capacity(tilemap.size().x as usize * tilemap.size().y as usize);

        let walkable = |x: i32, y: i32| {
            if x < 0 || y < 0 || x >= tilemap.size().x as i32 || y >= tilemap.size().y as i32 {
                return false;
            }

            let Some(tile) = tilemap.get(x as u32, y as u32) else {
                return false;
            };

            let collision = *collisions.get(tile.x as usize, tile.y as usize);
            (collision & blocked_layers_mask) == 0
        };

        for y in 0..tilemap.size().y as i32 {
            for x in 0..tilemap.size().x as i32 {
                if !walkable(x, y) {
                    continue;
                }

                let (tile_min_object, tile_max_object) =
                    tilemap.tile_to_object_bounds(UVec2::new(x as u32, y as u32));

                let tile_top_left = transform
                    .object_to_world
                    .transform_point3(tile_min_object)
                    .truncate();
                let tile_bottom_right = transform
                    .object_to_world
                    .transform_point3(tile_max_object)
                    .truncate();
                let tile_top_right = vec2(tile_bottom_right.x, tile_top_left.y);
                let tile_bottom_left = vec2(tile_top_left.x, tile_bottom_right.y);

                base_quads.push((
                    tile_top_left + Vec2::splat(R),
                    tile_bottom_right - Vec2::splat(R),
                ));

                let top_walkable = walkable(x, y - 1);
                let top_right_walkable = walkable(x + 1, y - 1);
                let right_walkable = walkable(x + 1, y);
                let bottom_right_walkable = walkable(x + 1, y + 1);
                let bottom_walkable = walkable(x, y + 1);
                let bottom_left_walkable = walkable(x - 1, y + 1);
                let left_walkable = walkable(x - 1, y);
                let top_left_walkable = walkable(x - 1, y - 1);

                if top_walkable {
                    base_quads.push((tile_top_left + vec2(R, 0.0), tile_top_right + vec2(-R, R)));
                }
                if right_walkable {
                    base_quads.push((
                        tile_top_right + vec2(-R, R),
                        tile_bottom_right + vec2(0.0, -R),
                    ));
                }
                if bottom_walkable {
                    base_quads.push((
                        tile_bottom_left + vec2(R, -R),
                        tile_bottom_right + vec2(-R, 0.0),
                    ));
                }
                if left_walkable {
                    base_quads.push((tile_top_left + vec2(0.0, R), tile_bottom_left + vec2(R, -R)));
                }

                if top_walkable && top_right_walkable && right_walkable {
                    base_quads.push((
                        tile_top_right + vec2(-R, 0.0),
                        tile_top_right + vec2(0.0, R),
                    ));
                }
                if right_walkable && bottom_right_walkable && bottom_walkable {
                    base_quads.push((
                        tile_bottom_right + vec2(-R, -R),
                        tile_bottom_right + vec2(0.0, 0.0),
                    ));
                }
                if bottom_walkable && bottom_left_walkable && left_walkable {
                    base_quads.push((
                        tile_bottom_left + vec2(0.0, -R),
                        tile_bottom_left + vec2(R, 0.0),
                    ));
                }
                if left_walkable && top_left_walkable && top_walkable {
                    base_quads.push((tile_top_left + vec2(0.0, 0.0), tile_top_left + vec2(R, R)));
                }
            }
        }

        // 1) Greedy rectangle extraction on a rectilinear cell grid induced by base quad boundaries.
        let mut xs = Vec::with_capacity(base_quads.len() * 2);
        let mut ys = Vec::with_capacity(base_quads.len() * 2);
        for (min, max) in &base_quads {
            xs.push(min.x);
            xs.push(max.x);
            ys.push(min.y);
            ys.push(max.y);
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        ys.sort_by(|a, b| a.total_cmp(b));
        xs.dedup_by(|a, b| (*a - *b).abs() <= EPS);
        ys.dedup_by(|a, b| (*a - *b).abs() <= EPS);
        let x_lines = xs;
        let y_lines = ys;
        let mut quads = Vec::new();

        if x_lines.len() >= 2 && y_lines.len() >= 2 {
            let cells_w = x_lines.len() - 1;
            let cells_h = y_lines.len() - 1;
            let mut occupied = vec![false; cells_w * cells_h];
            let cell_idx = |x: usize, y: usize| y * cells_w + x;

            for cy in 0..cells_h {
                for cx in 0..cells_w {
                    let center = vec2(
                        0.5 * (x_lines[cx] + x_lines[cx + 1]),
                        0.5 * (y_lines[cy] + y_lines[cy + 1]),
                    );
                    if base_quads.iter().any(|(min, max)| {
                        center.x >= min.x - EPS
                            && center.x <= max.x + EPS
                            && center.y >= min.y - EPS
                            && center.y <= max.y + EPS
                    }) {
                        occupied[cell_idx(cx, cy)] = true;
                    }
                }
            }

            // Greedy maximal-rectangle extraction on the conforming cell grid.
            let mut used = vec![false; cells_w * cells_h];
            let mut merged = Vec::new();
            for y in 0..cells_h {
                for x in 0..cells_w {
                    if !occupied[cell_idx(x, y)] || used[cell_idx(x, y)] {
                        continue;
                    }

                    let mut x1 = x + 1;
                    while x1 < cells_w && occupied[cell_idx(x1, y)] && !used[cell_idx(x1, y)] {
                        x1 += 1;
                    }

                    let mut y1 = y + 1;
                    'grow: while y1 < cells_h {
                        for xx in x..x1 {
                            if !occupied[cell_idx(xx, y1)] || used[cell_idx(xx, y1)] {
                                break 'grow;
                            }
                        }
                        y1 += 1;
                    }

                    for yy in y..y1 {
                        for xx in x..x1 {
                            used[cell_idx(xx, yy)] = true;
                        }
                    }

                    merged.push((vec2(x_lines[x], y_lines[y]), vec2(x_lines[x1], y_lines[y1])));
                }
            }

            // 2) Split touching neighbors until all shared borders are full-edge.
            let mut rects = merged;
            let mut changed = true;
            while changed {
                changed = false;
                'pairs: for i in 0..rects.len() {
                    for j in (i + 1)..rects.len() {
                        let (a_min, a_max) = rects[i];
                        let (b_min, b_max) = rects[j];

                        let split_horizontally = |target: (Vec2, Vec2), cut_y: f32| {
                            let (min, max) = target;
                            ((min, vec2(max.x, cut_y)), (vec2(min.x, cut_y), max))
                        };
                        let split_vertically = |target: (Vec2, Vec2), cut_x: f32| {
                            let (min, max) = target;
                            ((min, vec2(cut_x, max.y)), (vec2(cut_x, min.y), max))
                        };

                        let side_touch_x =
                            (a_max.x - b_min.x).abs() <= EPS || (b_max.x - a_min.x).abs() <= EPS;
                        if side_touch_x {
                            let overlap_min = a_min.y.max(b_min.y);
                            let overlap_max = a_max.y.min(b_max.y);
                            if overlap_max - overlap_min > EPS {
                                if b_min.y > a_min.y + EPS && b_min.y < a_max.y - EPS {
                                    let (lower, upper) =
                                        split_horizontally((a_min, a_max), b_min.y);
                                    rects[i] = lower;
                                    rects.push(upper);
                                    changed = true;
                                    break 'pairs;
                                }
                                if b_max.y > a_min.y + EPS && b_max.y < a_max.y - EPS {
                                    let (lower, upper) =
                                        split_horizontally((a_min, a_max), b_max.y);
                                    rects[i] = lower;
                                    rects.push(upper);
                                    changed = true;
                                    break 'pairs;
                                }
                                if a_min.y > b_min.y + EPS && a_min.y < b_max.y - EPS {
                                    let (lower, upper) =
                                        split_horizontally((b_min, b_max), a_min.y);
                                    rects[j] = lower;
                                    rects.push(upper);
                                    changed = true;
                                    break 'pairs;
                                }
                                if a_max.y > b_min.y + EPS && a_max.y < b_max.y - EPS {
                                    let (lower, upper) =
                                        split_horizontally((b_min, b_max), a_max.y);
                                    rects[j] = lower;
                                    rects.push(upper);
                                    changed = true;
                                    break 'pairs;
                                }
                            }
                        }

                        let side_touch_y =
                            (a_max.y - b_min.y).abs() <= EPS || (b_max.y - a_min.y).abs() <= EPS;
                        if side_touch_y {
                            let overlap_min = a_min.x.max(b_min.x);
                            let overlap_max = a_max.x.min(b_max.x);
                            if overlap_max - overlap_min > EPS {
                                if b_min.x > a_min.x + EPS && b_min.x < a_max.x - EPS {
                                    let (left, right) = split_vertically((a_min, a_max), b_min.x);
                                    rects[i] = left;
                                    rects.push(right);
                                    changed = true;
                                    break 'pairs;
                                }
                                if b_max.x > a_min.x + EPS && b_max.x < a_max.x - EPS {
                                    let (left, right) = split_vertically((a_min, a_max), b_max.x);
                                    rects[i] = left;
                                    rects.push(right);
                                    changed = true;
                                    break 'pairs;
                                }
                                if a_min.x > b_min.x + EPS && a_min.x < b_max.x - EPS {
                                    let (left, right) = split_vertically((b_min, b_max), a_min.x);
                                    rects[j] = left;
                                    rects.push(right);
                                    changed = true;
                                    break 'pairs;
                                }
                                if a_max.x > b_min.x + EPS && a_max.x < b_max.x - EPS {
                                    let (left, right) = split_vertically((b_min, b_max), a_max.x);
                                    rects[j] = left;
                                    rects.push(right);
                                    changed = true;
                                    break 'pairs;
                                }
                            }
                        }
                    }
                }
            }

            quads = rects;
        }

        // 3) Split each rectangle into two triangles for rendering/path queries.
        let triangles = quads
            .into_iter()
            .flat_map(|q| {
                let a = q.0;
                let d = q.1;
                let b = vec2(a.x, d.y);
                let c = vec2(d.x, a.y);
                vec![[a, c, b], [b, c, d]]
            })
            .collect();

        NavigationComponent {
            mesh: NavigationMesh::new(triangles),
            debug_path_triangles: Vec::new(),
            debug_path_points: Vec::new(),
        }
    }
}

impl NavigationMesh {
    /// Builds navmesh caches used during runtime path queries.
    pub fn new(triangles: Vec<[Vec2; 3]>) -> Self {
        let triangle_centers = triangles
            .iter()
            .map(Self::triangle_center)
            .collect::<Vec<_>>();
        let adjacency = Self::build_adjacency_from_triangles(&triangles);
        let spatial_cell_size = 64.0;
        let spatial_bins = Self::build_spatial_bins(&triangles, spatial_cell_size);
        Self {
            triangles,
            triangle_centers,
            adjacency,
            spatial_bins,
            spatial_cell_size,
        }
    }

    /// Returns center point of a triangle.
    fn triangle_center(triangle: &[Vec2; 3]) -> Vec2 {
        (triangle[0] + triangle[1] + triangle[2]) / 3.0
    }

    /// Tests if a point is inside a triangle.
    fn contains_point(triangle: &[Vec2; 3], point: Vec2) -> bool {
        let s1 = Self::signed_side_2d(point, triangle[0], triangle[1]);
        let s2 = Self::signed_side_2d(point, triangle[1], triangle[2]);
        let s3 = Self::signed_side_2d(point, triangle[2], triangle[0]);
        let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0;
        let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0;
        !(has_neg && has_pos)
    }

    /// Signed side test for point `p1` against directed edge `p2 -> p3`.
    ///
    /// Positive means left side, negative means right side, zero means collinear.
    fn signed_side_2d(p1: Vec2, p2: Vec2, p3: Vec2) -> f32 {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    }

    /// Finds the triangle containing the point using a spatial hash.
    fn triangle_at(&self, point: Vec2) -> Option<usize> {
        let key = self.spatial_key(point);
        if let Some(candidates) = self.spatial_bins.get(&key) {
            for idx in candidates {
                if Self::contains_point(&self.triangles[*idx], point) {
                    return Some(*idx);
                }
            }
        }
        // Fallback for points close to cell borders.
        self.triangles
            .iter()
            .enumerate()
            .find_map(|(idx, triangle)| Self::contains_point(triangle, point).then_some(idx))
            .or_else(|| {
                self.triangle_centers
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        a.distance_squared(point)
                            .total_cmp(&b.distance_squared(point))
                    })
                    .map(|(idx, _)| idx)
            })
    }

    /// Returns true when two triangles share an edge.
    fn triangles_adjacent(a: &[Vec2; 3], b: &[Vec2; 3]) -> bool {
        const EPS: f32 = 0.001;
        let mut shared = 0usize;
        for pa in a {
            for pb in b {
                if pa.distance_squared(*pb) <= EPS * EPS {
                    shared += 1;
                    break;
                }
            }
        }
        shared >= 2
    }

    /// Precomputes nav graph adjacency once when navmesh is created.
    fn build_adjacency_from_triangles(triangles: &[[Vec2; 3]]) -> Vec<Vec<usize>> {
        let mut adjacency = vec![Vec::new(); triangles.len()];
        for i in 0..triangles.len() {
            for j in (i + 1)..triangles.len() {
                if Self::triangles_adjacent(&triangles[i], &triangles[j]) {
                    adjacency[i].push(j);
                    adjacency[j].push(i);
                }
            }
        }
        adjacency
    }

    /// Builds a spatial hash that maps grid cells to intersecting triangles.
    fn build_spatial_bins(
        triangles: &[[Vec2; 3]],
        cell_size: f32,
    ) -> HashMap<(i32, i32), Vec<usize>> {
        let mut bins: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (idx, triangle) in triangles.iter().enumerate() {
            let min_x = triangle.iter().map(|v| v.x).fold(f32::INFINITY, f32::min);
            let min_y = triangle.iter().map(|v| v.y).fold(f32::INFINITY, f32::min);
            let max_x = triangle
                .iter()
                .map(|v| v.x)
                .fold(f32::NEG_INFINITY, f32::max);
            let max_y = triangle
                .iter()
                .map(|v| v.y)
                .fold(f32::NEG_INFINITY, f32::max);
            let cx0 = (min_x / cell_size).floor() as i32;
            let cy0 = (min_y / cell_size).floor() as i32;
            let cx1 = (max_x / cell_size).floor() as i32;
            let cy1 = (max_y / cell_size).floor() as i32;
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    bins.entry((cx, cy)).or_default().push(idx);
                }
            }
        }
        bins
    }

    /// Maps world position to spatial grid key.
    fn spatial_key(&self, point: Vec2) -> (i32, i32) {
        (
            (point.x / self.spatial_cell_size).floor() as i32,
            (point.y / self.spatial_cell_size).floor() as i32,
        )
    }

    /// Finds a path on navmesh and returns waypoints after funnel + LOS shortcutting.
    pub fn find_path(&self, start: Vec2, end: Vec2) -> Option<NavigationPath> {
        let start_tri = self.triangle_at(start)?;
        let end_tri = self.triangle_at(end)?;

        if start_tri == end_tri {
            return Some(NavigationPath {
                waypoints: vec![end],
                triangle_indices: vec![start_tri],
            });
        }

        let triangle_path = Self::a_star_triangle_path(
            start_tri,
            end_tri,
            &self.adjacency,
            &self.triangle_centers,
        )?;
        let pulled = self.string_pull(start, end, &triangle_path);
        let waypoints = self.shortcut_path(&pulled, &triangle_path);

        Some(NavigationPath {
            waypoints,
            triangle_indices: triangle_path,
        })
    }

    fn a_star_triangle_path(
        start: usize,
        goal: usize,
        adjacency: &[Vec<usize>],
        centers: &[Vec2],
    ) -> Option<Vec<usize>> {
        #[derive(Copy, Clone)]
        struct QueueNode {
            node: usize,
            f: f32,
        }
        impl Eq for QueueNode {}
        impl PartialEq for QueueNode {
            fn eq(&self, other: &Self) -> bool {
                self.f == other.f && self.node == other.node
            }
        }
        impl Ord for QueueNode {
            fn cmp(&self, other: &Self) -> Ordering {
                other
                    .f
                    .total_cmp(&self.f)
                    .then_with(|| other.node.cmp(&self.node))
            }
        }
        impl PartialOrd for QueueNode {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut open = BinaryHeap::new();
        let mut came_from: Vec<Option<usize>> = vec![None; centers.len()];
        let mut g_score = vec![f32::INFINITY; centers.len()];
        let mut closed = HashSet::new();

        g_score[start] = 0.0;
        open.push(QueueNode {
            node: start,
            f: centers[start].distance(centers[goal]),
        });

        while let Some(current) = open.pop() {
            if current.node == goal {
                let mut path = vec![goal];
                let mut cursor = goal;
                while let Some(prev) = came_from[cursor] {
                    path.push(prev);
                    cursor = prev;
                }
                path.reverse();
                return Some(path);
            }
            if !closed.insert(current.node) {
                continue;
            }
            for neighbor in &adjacency[current.node] {
                if closed.contains(neighbor) {
                    continue;
                }
                let tentative =
                    g_score[current.node] + centers[current.node].distance(centers[*neighbor]);
                if tentative < g_score[*neighbor] {
                    came_from[*neighbor] = Some(current.node);
                    g_score[*neighbor] = tentative;
                    let h = centers[*neighbor].distance(centers[goal]);
                    open.push(QueueNode {
                        node: *neighbor,
                        f: tentative + h,
                    });
                }
            }
        }
        None
    }

    fn shared_edge(a: &[Vec2; 3], b: &[Vec2; 3]) -> Option<(Vec2, Vec2)> {
        const EPS: f32 = 0.001;
        let mut shared = Vec::new();
        for pa in a {
            for pb in b {
                if pa.distance_squared(*pb) <= EPS * EPS {
                    shared.push(*pa);
                    break;
                }
            }
        }
        if shared.len() >= 2 {
            Some((shared[0], shared[1]))
        } else {
            None
        }
    }

    /// Returns the 2D cross-product scalar for triangle `(a, b, c)`.
    ///
    /// The value equals `2 * area(a,b,c)` with sign:
    /// - `> 0`: counter-clockwise / left turn
    /// - `< 0`: clockwise / right turn
    /// - `= 0`: collinear
    ///
    /// Funnel pathing uses this to keep and update left/right portal bounds.
    fn tri_area2(a: Vec2, b: Vec2, c: Vec2) -> f32 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }

    /// Runs the funnel algorithm over triangle portals.
    fn string_pull(&self, start: Vec2, end: Vec2, triangle_path: &[usize]) -> Vec<Vec2> {
        if triangle_path.is_empty() {
            return vec![end];
        }

        let mut portals: Vec<(Vec2, Vec2)> = Vec::with_capacity(triangle_path.len() + 1);
        portals.push((start, start));
        for pair in triangle_path.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let Some((p, q)) = Self::shared_edge(&self.triangles[a], &self.triangles[b]) else {
                continue;
            };
            portals.push((p, q));
        }
        portals.push((end, end));

        let mut out = Vec::new();
        out.push(start);

        let mut apex = start;
        let mut left = portals[0].0;
        let mut right = portals[0].1;
        let mut apex_index = 0usize;
        let mut left_index = 0usize;
        let mut right_index = 0usize;
        let mut i = 1usize;

        while i < portals.len() {
            let (mut new_left, mut new_right) = portals[i];
            if Self::tri_area2(apex, new_left, new_right) < 0.0 {
                std::mem::swap(&mut new_left, &mut new_right);
            }

            if Self::tri_area2(apex, right, new_right) <= 0.0 {
                if apex == right || Self::tri_area2(apex, left, new_right) > 0.0 {
                    right = new_right;
                    right_index = i;
                } else {
                    out.push(left);
                    apex = left;
                    apex_index = left_index;
                    left = apex;
                    right = apex;
                    left_index = apex_index;
                    right_index = apex_index;
                    i = apex_index + 1;
                    continue;
                }
            }

            if Self::tri_area2(apex, left, new_left) >= 0.0 {
                if apex == left || Self::tri_area2(apex, right, new_left) < 0.0 {
                    left = new_left;
                    left_index = i;
                } else {
                    out.push(right);
                    apex = right;
                    apex_index = right_index;
                    left = apex;
                    right = apex;
                    left_index = apex_index;
                    right_index = apex_index;
                    i = apex_index + 1;
                    continue;
                }
            }

            i += 1;
        }

        if out.last().is_none_or(|p| p.distance_squared(end) > 0.0001) {
            out.push(end);
        }
        out
    }

    /// Greedy line-of-sight shortcutting over a funnel-produced path.
    fn shortcut_path(&self, path: &[Vec2], corridor: &[usize]) -> Vec<Vec2> {
        if path.len() <= 2 {
            return path.to_vec();
        }
        let mut out = Vec::new();
        let mut i = 0usize;
        out.push(path[0]);
        while i < path.len() - 1 {
            let mut best = i + 1;
            for j in ((i + 1)..path.len()).rev() {
                if self.segment_inside_corridor(path[i], path[j], corridor) {
                    best = j;
                    break;
                }
            }
            out.push(path[best]);
            i = best;
        }
        out
    }

    /// Returns true if the whole segment stays inside the triangle corridor.
    fn segment_inside_corridor(&self, a: Vec2, b: Vec2, corridor: &[usize]) -> bool {
        if corridor.is_empty() {
            return true;
        }
        let dist = a.distance(b);
        let samples = ((dist / 8.0).ceil() as usize).max(2);
        for s in 0..=samples {
            let t = s as f32 / samples as f32;
            let p = a.lerp(b, t);
            let inside = corridor.iter().any(|idx| {
                self.triangles
                    .get(*idx)
                    .is_some_and(|tri| Self::contains_point(tri, p))
            });
            if !inside {
                return false;
            }
        }
        true
    }
}

impl Component for NavigationComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::NEEDS_DEBUG_DRAW
    }

    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    ) {
        if *ctx.shown_debug_layers & (1 << 3) == 0 {
            return;
        }

        let path_triangles: HashSet<usize> = self.debug_path_triangles.iter().copied().collect();
        render_data
            .debug_drawables
            .extend(self.mesh.triangles.iter().map(|t| DebugDrawableInstance {
                object_to_world: Mat4::IDENTITY,
                depth: 0.0,
                color: Color::GREEN.to_vec4(),
                vertices: vec![t[0], t[1], t[1], t[2], t[2], t[0]],
                layer: 1 << 3,
                space: SpriteSpace::World,
            }));

        render_data.debug_drawables.extend(
            path_triangles
                .iter()
                .filter_map(|idx| self.mesh.triangles.get(*idx))
                .map(|t| DebugDrawableInstance {
                    object_to_world: Mat4::IDENTITY,
                    depth: 0.0,
                    color: Color::YELLOW.to_vec4(),
                    vertices: vec![t[0], t[1], t[1], t[2], t[2], t[0]],
                    layer: 1 << 3,
                    space: SpriteSpace::World,
                }),
        );

        if self.debug_path_points.len() >= 2 {
            let mut vertices = Vec::with_capacity((self.debug_path_points.len() - 1) * 2);
            for segment in self.debug_path_points.windows(2) {
                vertices.push(segment[0]);
                vertices.push(segment[1]);
            }
            render_data.debug_drawables.push(DebugDrawableInstance {
                object_to_world: Mat4::IDENTITY,
                depth: 0.0,
                color: Color::BLUE.to_vec4(),
                vertices,
                layer: 1 << 3,
                space: SpriteSpace::World,
            });
        }
    }
}
