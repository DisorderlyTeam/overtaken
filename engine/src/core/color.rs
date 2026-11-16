/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};

/// Represents a color in the linear color space.
///
/// Represented as a [`Vec4`], so four `f32` components, in order: red, green, blue, alpha.
#[derive(Debug, Copy, Clone, PartialEq, Default, Pod, Zeroable)]
#[repr(transparent)]
pub struct Color(Vec4);

impl Color {
    // Constants
    pub const TRANSPARENT: Self = Self::from_vec4(Vec4::ZERO);
    pub const BLACK: Self = Self::from_vec4(Vec4::new(0.0, 0.0, 0.0, 1.0));
    pub const WHITE: Self = Self::from_vec4(Vec4::new(1.0, 1.0, 1.0, 1.0));
    pub const RED: Self = Self::from_vec4(Vec4::new(1.0, 0.0, 0.0, 1.0));
    pub const GREEN: Self = Self::from_vec4(Vec4::new(0.0, 1.0, 0.0, 1.0));
    pub const BLUE: Self = Self::from_vec4(Vec4::new(0.0, 0.0, 1.0, 1.0));
    pub const YELLOW: Self = Self::from_vec4(Vec4::new(1.0, 1.0, 0.0, 1.0));
    pub const CYAN: Self = Self::from_vec4(Vec4::new(0.0, 1.0, 1.0, 1.0));
    pub const MAGENTA: Self = Self::from_vec4(Vec4::new(1.0, 0.0, 1.0, 1.0));

    // from_x
    #[inline]
    pub const fn from_vec4(color: Vec4) -> Self {
        Self(color)
    }

    #[inline]
    pub const fn from_vec3(color: Vec3) -> Self {
        Self(Vec4::new(color.x, color.y, color.z, 1.0))
    }

    #[inline]
    pub const fn from_rgba_f32(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self(Vec4::new(r, g, b, a))
    }

    #[inline]
    pub const fn from_rgb_f32(r: f32, g: f32, b: f32) -> Self {
        Self(Vec4::new(r, g, b, 1.0))
    }

    #[inline]
    pub const fn from_rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(Vec4::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ))
    }

    #[inline]
    pub const fn from_rgb_u8(r: u8, g: u8, b: u8) -> Self {
        Self(Vec4::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            1.0,
        ))
    }

    #[inline]
    pub const fn from_int_rgba(color: u32) -> Self {
        let bytes = color.to_be_bytes();
        Self(Vec4::new(
            bytes[1] as f32 / 255.0,
            bytes[2] as f32 / 255.0,
            bytes[3] as f32 / 255.0,
            bytes[0] as f32 / 255.0,
        ))
    }

    #[inline]
    pub const fn from_int_rgb(color: u32) -> Self {
        let bytes = color.to_be_bytes();
        debug_assert!(
            bytes[0] == 0,
            "RGB color int should not contain alpha channel"
        );
        Self(Vec4::new(
            bytes[1] as f32 / 255.0,
            bytes[2] as f32 / 255.0,
            bytes[3] as f32 / 255.0,
            1.0,
        ))
    }

    #[inline]
    pub const fn from_wgpu(color: wgpu::Color) -> Self {
        Self(Vec4::new(
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a as f32 / 255.0,
        ))
    }

    // to_x
    #[inline]
    pub const fn to_vec4(&self) -> Vec4 {
        self.0
    }

    #[inline]
    pub fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.0.x, self.0.y, self.0.z)
    }

    #[inline]
    pub fn to_rgba_f32(&self) -> (f32, f32, f32, f32) {
        (self.0.x, self.0.y, self.0.z, self.0.w)
    }

    #[inline]
    pub fn to_rgb_f32(&self) -> (f32, f32, f32) {
        (self.0.x, self.0.y, self.0.z)
    }

    #[inline]
    pub fn to_rgba_u8(&self) -> (u8, u8, u8, u8) {
        (
            (self.0.x * 255.0) as u8,
            (self.0.y * 255.0) as u8,
            (self.0.z * 255.0) as u8,
            (self.0.w * 255.0) as u8,
        )
    }

    #[inline]
    pub fn to_rgb_u8(&self) -> (u8, u8, u8) {
        (
            (self.0.x * 255.0) as u8,
            (self.0.y * 255.0) as u8,
            (self.0.z * 255.0) as u8,
        )
    }

    #[inline]
    pub fn to_wgpu(&self) -> wgpu::Color {
        wgpu::Color {
            r: (self.0.x * 255.0) as f64,
            g: (self.0.y * 255.0) as f64,
            b: (self.0.z * 255.0) as f64,
            a: (self.0.w * 255.0) as f64,
        }
    }

    // Getters
    #[inline]
    pub fn r(&self) -> f32 {
        self.0.x
    }

    #[inline]
    pub fn g(&self) -> f32 {
        self.0.y
    }

    #[inline]
    pub fn b(&self) -> f32 {
        self.0.z
    }

    #[inline]
    pub fn a(&self) -> f32 {
        self.0.w
    }
}
