/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::core::math::{RangeExt, intersect};
use crate::render_world::{
    DEBUG_DRAW_LAYER_COLLISIONS, DebugDrawableInstance, DebugShape, SpriteSpace,
};
use glam::{Mat4, Quat, Vec2, Vec3Swizzles, Vec4, vec2, vec4};
use tracing::warn;

pub const DEFAULT_COLLISION_LAYER: u32 = 1 << 0;
pub const DEFAULT_COLLIDES_WITH: u32 = u32::MAX;

#[derive(Debug, Copy, Clone)]
pub enum CollisionShape {
    Circle { position: Vec2, radius: f32 },
    Rect { position: Vec2, size: Vec2 },
}

impl CollisionShape {
    pub fn transform(&self, transform: Mat4) -> Self {
        let (scale, rot, trans) = transform.to_scale_rotation_translation();
        if cfg!(debug_assertions) {
            if rot != Quat::IDENTITY {
                warn!(?self, ?transform, "collision shape shouldn't have rotation");
            }
        }
        match self {
            CollisionShape::Circle { position, radius } => {
                if cfg!(debug_assertions) {
                    if scale.x != scale.y {
                        warn!(
                            ?self,
                            ?transform,
                            "circle collision shape has non-uniform scaling"
                        );
                    }
                }

                CollisionShape::Circle {
                    position: position + trans.xy(),
                    radius: radius * scale.x,
                }
            }
            CollisionShape::Rect { position, size } => CollisionShape::Rect {
                position: position + trans.xy(),
                size: size * scale.xy(),
            },
        }
    }

    pub fn from_rect_bounds(a: Vec2, b: Vec2) -> CollisionShape {
        let position = a.midpoint(b);
        let size = (b - a).abs();

        CollisionShape::Rect { position, size }
    }

    pub fn position(&self) -> Vec2 {
        match self {
            CollisionShape::Circle { position, .. } => *position,
            CollisionShape::Rect { position, .. } => *position,
        }
    }

    /// Gets the debug shape that may be used to represent this collision.
    ///
    /// It encodes size, but it *does not* encode position.
    pub fn to_debug_shape(&self) -> DebugShape {
        match self {
            CollisionShape::Circle { radius, .. } => DebugShape::Circle {
                radius: *radius,
                segments: 10,
            },
            CollisionShape::Rect { size, .. } => DebugShape::Rect {
                width: size.x,
                height: size.y,
            },
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct StaticCollider {
    pub shape: CollisionShape,
    pub layer: u32,
    pub collides_with: u32,
}

impl StaticCollider {
    pub fn new(shape: CollisionShape) -> Self {
        Self {
            shape,
            layer: DEFAULT_COLLISION_LAYER,
            collides_with: DEFAULT_COLLIDES_WITH,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct DynamicCollider {
    pub shape: CollisionShape,
    pub weight: f32,
    pub layer: u32,
    pub collides_with: u32,
}

impl DynamicCollider {
    pub fn new(shape: CollisionShape, weight: f32) -> Self {
        Self {
            shape,
            weight,
            layer: DEFAULT_COLLISION_LAYER,
            collides_with: DEFAULT_COLLIDES_WITH,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TriggerCollider {
    pub shape: CollisionShape,
    pub layer: u32,
    pub collides_with: u32,
}

impl TriggerCollider {
    pub fn new(shape: CollisionShape) -> Self {
        Self {
            shape,
            layer: DEFAULT_COLLISION_LAYER,
            collides_with: DEFAULT_COLLIDES_WITH,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Collider {
    Static(StaticCollider),
    Dynamic(DynamicCollider),
    Trigger(TriggerCollider),
}

impl Collider {
    pub fn shape(&self) -> CollisionShape {
        match self {
            Collider::Static(StaticCollider { shape, .. }) => *shape,
            Collider::Dynamic(DynamicCollider { shape, .. }) => *shape,
            Collider::Trigger(TriggerCollider { shape, .. }) => *shape,
        }
    }

    pub fn is_static(&self) -> bool {
        match self {
            Collider::Static(_) => true,
            Collider::Dynamic(_) => false,
            Collider::Trigger(_) => true,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        !self.is_static()
    }

    pub fn is_trigger(&self) -> bool {
        matches!(self, Collider::Trigger(_))
    }

    pub fn weight(&self) -> f32 {
        match self {
            Collider::Static(_) => f32::INFINITY,
            Collider::Dynamic(DynamicCollider { weight, .. }) => *weight,
            Collider::Trigger(_) => f32::INFINITY,
        }
    }

    pub fn layer(&self) -> u32 {
        match self {
            Collider::Static(StaticCollider { layer, .. }) => *layer,
            Collider::Dynamic(DynamicCollider { layer, .. }) => *layer,
            Collider::Trigger(TriggerCollider { layer, .. }) => *layer,
        }
    }

    pub fn collides_with(&self) -> u32 {
        match self {
            Collider::Static(StaticCollider { collides_with, .. }) => *collides_with,
            Collider::Dynamic(DynamicCollider { collides_with, .. }) => *collides_with,
            Collider::Trigger(TriggerCollider { collides_with, .. }) => *collides_with,
        }
    }

    pub fn can_collide_with(&self, other: &Collider) -> bool {
        (self.collides_with() & other.layer()) != 0 && (other.collides_with() & self.layer()) != 0
    }

    pub fn to_debug_drawable(&self, object_to_world: Mat4) -> DebugDrawableInstance {
        let shape = self.shape();
        let color: Vec4 = if self.is_trigger() {
            vec4(1.0, 1.0, 0.0, 1.0)
        } else if self.is_dynamic() {
            vec4(0.0, 1.0, 0.0, 1.0)
        } else {
            vec4(1.0, 0.0, 0.0, 1.0)
        };

        let position = shape.position();
        let debug_shape = shape.to_debug_shape();

        let object_to_world = Mat4::from_translation(position.extend(1000.0)) * object_to_world;

        DebugDrawableInstance {
            object_to_world,
            depth: 1000.0,
            color,
            vertices: debug_shape.outline_vertices(),
            space: SpriteSpace::World,
            layer: DEBUG_DRAW_LAYER_COLLISIONS,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Collision {
    contact_point: Vec2,
    normal: Vec2,
    penetration: f32,
}

impl Collision {
    pub fn normal(&self) -> Vec2 {
        self.normal
    }

    pub fn penetration(&self) -> f32 {
        self.penetration
    }

    /// Tries to detect collision between shapes `a` and `b`.
    ///
    /// If a collision is detected, returns [Collision] object with:
    /// - a contact point,
    /// - collision's normal from A to B (B to A is inverse of that),
    /// - penetration (0 if the shapes are just touching, increasing).
    /// If no collision is detected, returns `None`.
    ///
    /// Both collision shapes should be in the same space. Preferably transformed
    /// to world space with [CollisionShape::transform].
    pub fn between(a: CollisionShape, b: CollisionShape) -> Option<Collision> {
        use CollisionShape::*;

        match (a, b) {
            (
                Rect {
                    position: a_pos,
                    size: a_size,
                },
                Rect {
                    position: b_pos,
                    size: b_size,
                },
            ) => {
                let a_p1 = a_pos - a_size / 2.0;
                let a_p2 = a_pos + a_size / 2.0;
                let b_p1 = b_pos - b_size / 2.0;
                let b_p2 = b_pos + b_size / 2.0;
                let a_cx = (a_p1.x + a_p2.x) / 2.0;
                let a_cy = (a_p1.y + a_p2.y) / 2.0;
                let b_cx = (b_p1.x + b_p2.x) / 2.0;
                let b_cy = (b_p1.y + b_p2.y) / 2.0;
                let x_norm = (b_cx - a_cx).signum();
                let y_norm = (b_cy - a_cy).signum();
                // We check whether AABB-s intersect
                // For the boxes to intersect, all coordinates must intersect
                let x_overlap = intersect(a_p1.x..=a_p2.x, b_p1.x..=b_p2.x);
                let y_overlap = intersect(a_p1.y..=a_p2.y, b_p1.y..=b_p2.y);

                if let Some(x_overlap) = x_overlap
                    && let Some(y_overlap) = y_overlap
                {
                    let contact_point = Vec2::new(x_overlap.middle(), y_overlap.middle());
                    let x_span = x_overlap.span();
                    let y_span = y_overlap.span();

                    if x_span <= y_span {
                        Some(Collision {
                            contact_point,
                            normal: Vec2::new(x_norm, 0.0),
                            penetration: x_span,
                        })
                    } else {
                        Some(Collision {
                            contact_point,
                            normal: Vec2::new(0.0, y_norm),
                            penetration: y_span,
                        })
                    }
                } else {
                    // No overlap
                    None
                }
            }
            (
                Circle {
                    position: a_pos,
                    radius: a_rad,
                },
                Circle {
                    position: b_pos,
                    radius: b_rad,
                },
            ) => {
                let pitch = a_pos.distance(b_pos);
                let rad_sum = a_rad + b_rad;
                let overlap = rad_sum - pitch;
                if overlap <= 0.0 {
                    return None;
                }

                Some(Collision {
                    contact_point: a_pos.lerp(b_pos, b_rad / rad_sum),
                    normal: (b_pos - a_pos).normalize_or_zero(),
                    penetration: overlap,
                })
            }
            (
                Rect {
                    position: rect_pos,
                    size: rect_size,
                },
                Circle {
                    position: circle_pos,
                    radius,
                },
            ) => {
                let half_size = rect_size / 2.0;
                let rect_min = rect_pos - half_size;
                let rect_max = rect_pos + half_size;

                let closest_point = circle_pos.clamp(rect_min, rect_max);
                let to_circle = circle_pos - closest_point;
                let distance = to_circle.length();

                if distance > 0.0 {
                    let penetration = radius - distance;
                    if penetration <= 0.0 {
                        return None;
                    }

                    Some(Collision {
                        contact_point: closest_point,
                        normal: to_circle / distance,
                        penetration,
                    })
                } else {
                    let dx = half_size.x - (circle_pos.x - rect_pos.x).abs();
                    let dy = half_size.y - (circle_pos.y - rect_pos.y).abs();

                    let (normal, contact_point, inner_distance) = if dx <= dy {
                        if circle_pos.x >= rect_pos.x {
                            (vec2(1.0, 0.0), vec2(rect_max.x, circle_pos.y), dx)
                        } else {
                            (vec2(-1.0, 0.0), vec2(rect_min.x, circle_pos.y), dx)
                        }
                    } else if circle_pos.y >= rect_pos.y {
                        (vec2(0.0, 1.0), vec2(circle_pos.x, rect_max.y), dy)
                    } else {
                        (vec2(0.0, -1.0), vec2(circle_pos.x, rect_min.y), dy)
                    };

                    Some(Collision {
                        contact_point,
                        normal,
                        penetration: radius + inner_distance,
                    })
                }
            }
            (Circle { .. }, Rect { .. }) => {
                Collision::between(b, a).map(|collision| collision.inverted())
            }
        }
    }

    pub fn inverted(&self) -> Self {
        Collision {
            normal: -self.normal,
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::scene::collisions::{Collision, CollisionShape};
    use glam::vec2;
    use test_log::test;

    #[test]
    fn collide_rects() {
        let a = CollisionShape::from_rect_bounds(vec2(0.0, 0.0), vec2(10.0, 10.0));
        let b = CollisionShape::from_rect_bounds(vec2(2.0, 3.0), vec2(12.0, 13.0));

        let col = Collision::between(a, b);

        assert!(col.is_some());
        assert_eq!(col.as_ref().unwrap().penetration, 7.0);
        assert_eq!(col.unwrap().normal, vec2(0.0, 1.0));
    }

    #[test]
    fn collide_rects_normal_points_from_a_to_b() {
        let left = CollisionShape::Rect {
            position: vec2(0.0, 0.0),
            size: vec2(4.0, 4.0),
        };
        let right = CollisionShape::Rect {
            position: vec2(3.0, 0.0),
            size: vec2(4.0, 4.0),
        };

        let col_lr = Collision::between(left, right).unwrap();
        let col_rl = Collision::between(right, left).unwrap();

        assert_eq!(col_lr.normal, vec2(1.0, 0.0));
        assert_eq!(col_rl.normal, vec2(-1.0, 0.0));
    }

    #[test]
    fn dont_collide_rects() {
        let a = CollisionShape::from_rect_bounds(vec2(0.0, 0.0), vec2(2.0, 2.0));
        let b = CollisionShape::from_rect_bounds(vec2(3.0, 3.0), vec2(5.0, 5.0));

        let col = Collision::between(a, b);

        assert!(col.is_none());
    }

    #[test]
    fn collide_circles() {
        let a = CollisionShape::Circle {
            position: vec2(0.0, 0.0),
            radius: 1.0,
        };
        let b = CollisionShape::Circle {
            position: vec2(1.0, 0.0),
            radius: 1.0,
        };

        let col = Collision::between(a, b);

        assert!(col.is_some());
        assert_eq!(col.as_ref().unwrap().penetration, 1.0);
        assert_eq!(col.as_ref().unwrap().normal, vec2(1.0, 0.0));
    }

    #[test]
    fn dont_collide_circles() {
        let a = CollisionShape::Circle {
            position: vec2(0.0, 0.0),
            radius: 1.0,
        };
        let b = CollisionShape::Circle {
            position: vec2(3.0, 0.0),
            radius: 1.0,
        };

        let col = Collision::between(a, b);

        assert!(col.is_none());
    }

    #[test]
    fn collide_rect_circle() {
        let rect = CollisionShape::Rect {
            position: vec2(0.0, 0.0),
            size: vec2(4.0, 4.0),
        };
        let circle = CollisionShape::Circle {
            position: vec2(2.5, 0.0),
            radius: 1.0,
        };

        let col = Collision::between(rect, circle);

        assert!(col.is_some());
        assert_eq!(col.as_ref().unwrap().penetration, 0.5);
        assert_eq!(col.as_ref().unwrap().normal, vec2(1.0, 0.0));
        assert_eq!(col.as_ref().unwrap().contact_point, vec2(2.0, 0.0));
    }
}
