/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::assets::font::Font;
use crate::assets::{AssetHandle, AssetManager};
use crate::context::RenderCtx;
use crate::render_world::{SceneRenderData, TextRenderInstance};
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, LocalTransform, Scene};
use glyphon::Family;
use glyphon::cosmic_text::CacheKeyFlags;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use tracing::{debug_span, instrument, trace_span};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum TextAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    HorizonLeft,
    #[default]
    HorizonCenter,
    HorizonRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl TextAnchor {
    pub fn horizontal_align_factor(self) -> f32 {
        match self {
            Self::TopLeft | Self::HorizonLeft | Self::BottomLeft => 0.0,
            Self::TopCenter | Self::HorizonCenter | Self::BottomCenter => 0.5,
            Self::TopRight | Self::HorizonRight | Self::BottomRight => 1.0,
        }
    }

    pub fn vertical_align_factor(self) -> f32 {
        match self {
            Self::TopLeft | Self::TopCenter | Self::TopRight => 0.0,
            Self::HorizonLeft | Self::HorizonCenter | Self::HorizonRight => 0.5,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight => 1.0,
        }
    }

    pub(crate) fn to_glyphon(self) -> glyphon::cosmic_text::Align {
        match self {
            Self::TopLeft | Self::HorizonLeft | Self::BottomLeft => {
                glyphon::cosmic_text::Align::Left
            }
            Self::TopCenter | Self::HorizonCenter | Self::BottomCenter => {
                glyphon::cosmic_text::Align::Center
            }
            Self::TopRight | Self::HorizonRight | Self::BottomRight => {
                glyphon::cosmic_text::Align::Right
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TextStyle {
    pub font: AssetHandle<Font>,
    /// Line height override. If `None`, uses font's default line height.
    pub line_height: Option<f32>,
    /// Additional font scale multiplier. `1.0` keeps original size.
    pub scale: f32,
    pub color: [u8; 4],
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TextLayout {
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub anchor: TextAnchor,
}

impl Default for TextLayout {
    fn default() -> Self {
        Self {
            max_width: None,
            max_height: None,
            anchor: TextAnchor::HorizonCenter,
        }
    }
}

pub struct TextBufferState {
    buffer: Option<glyphon::Buffer>,
    dirty: bool,
    depth_key: usize,
}

impl Default for TextBufferState {
    fn default() -> Self {
        Self {
            buffer: None,
            dirty: true,
            depth_key: 0,
        }
    }
}

impl TextBufferState {
    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn buffer(&self) -> Option<&glyphon::Buffer> {
        self.buffer.as_ref()
    }

    #[instrument(
        level = "trace",
        name = "sync_text_buffer_state",
        skip(self, asset_manager, font_system)
    )]
    pub fn sync(
        &mut self,
        asset_manager: &AssetManager,
        font_system: &mut glyphon::FontSystem,
        text: &str,
        style: &TextStyle,
        layout: &TextLayout,
        depth_key: usize,
    ) {
        let font = asset_manager
            .resolve_handle(style.font)
            .expect("font handle valid");

        if self.buffer.is_none() {
            let font_scale = style.scale.max(0.001);
            self.buffer = Some(glyphon::Buffer::new(
                font_system,
                glyphon::Metrics::new(
                    font.metadata.font_size * font_scale,
                    style.line_height.unwrap_or(font.metadata.line_height) * font_scale,
                ),
            ));
            self.dirty = true;
        }

        let width = layout.max_width;
        let height = layout.max_height;
        let font_scale = style.scale.max(0.001);
        let base_line_height = style.line_height.unwrap_or(font.metadata.line_height);
        let metrics = glyphon::Metrics::new(
            font.metadata.font_size * font_scale,
            base_line_height * font_scale,
        );

        let buffer = self.buffer.as_mut().expect("buffer initialized");
        buffer.set_metrics_and_size(font_system, metrics, width, height);

        if self.depth_key != depth_key {
            self.dirty = true;
            self.depth_key = depth_key;
        }

        if self.dirty {
            let _span = trace_span!("sync_text_buffer_state_dirty").entered();
            let mut attrs = glyphon::Attrs::new()
                .family(Family::Name(&font.family_name))
                .cache_key_flags(CacheKeyFlags::PIXEL_FONT | CacheKeyFlags::DISABLE_HINTING);
            attrs = attrs.metadata(self.depth_key);
            buffer.set_text(
                font_system,
                text,
                &attrs,
                glyphon::Shaping::Advanced,
                Some(layout.anchor.to_glyphon()),
            );
            self.dirty = false;
        }
    }
}

#[derive(Clone, Default)]
pub struct TextBufferHandle(Rc<RefCell<TextBufferState>>);

impl TextBufferHandle {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(TextBufferState::default())))
    }

    pub fn borrow(&self) -> Ref<'_, TextBufferState> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, TextBufferState> {
        self.0.borrow_mut()
    }
}

#[derive(Clone)]
pub struct TextComponent {
    text: String,
    style: TextStyle,
    layout: TextLayout,
    buffer: TextBufferHandle,
}

impl Component for TextComponent {
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
        let world_z = global.object_to_world.transform_point3(glam::Vec3::ZERO).z;

        render_data.texts.push(TextRenderInstance {
            text: self.text().to_owned(), // TODO Maybe Arc to avoid allocation every frame?
            style: self.style(),
            layout: self.layout(),
            buffer: self.buffer_handle(),
            object_to_world: global.object_to_world,
            depth: world_z,
            space: local.space(),
        });
    }
}

impl TextComponent {
    pub fn new(text: impl Into<String>, style: TextStyle, layout: TextLayout) -> Self {
        Self {
            text: text.into(),
            style,
            layout,
            buffer: TextBufferHandle::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn style(&self) -> TextStyle {
        self.style
    }

    pub fn layout(&self) -> TextLayout {
        self.layout
    }

    pub fn buffer_handle(&self) -> TextBufferHandle {
        self.buffer.clone()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.text = text;
        self.mark_dirty();
    }

    pub fn set_style(&mut self, style: TextStyle) {
        self.style = style;
        self.mark_dirty();
    }

    pub fn set_layout(&mut self, layout: TextLayout) {
        self.layout = layout;
        self.mark_dirty();
    }

    pub fn update_style(&mut self, update: impl FnOnce(&mut TextStyle)) {
        update(&mut self.style);
        self.mark_dirty();
    }

    pub fn update_layout(&mut self, update: impl FnOnce(&mut TextLayout)) {
        update(&mut self.layout);
        self.mark_dirty();
    }

    pub fn mark_dirty(&mut self) {
        self.buffer.borrow_mut().mark_dirty();
    }
}
