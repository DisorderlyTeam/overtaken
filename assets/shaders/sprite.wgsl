/*
 * Copyright (c) 2026. DisorderlyTeam
 */

struct FrameUniform {
    world_to_clip: mat4x4f,
    render_resolution: vec2f,
    _pad: vec2f,
};

@group(0) @binding(0)
var<uniform> frame: FrameUniform;

@group(0) @binding(1)
var sprite_sampler: sampler;

@group(0) @binding(2)
var lighting_texture: texture_2d<f32>;

@group(1) @binding(0)
var sprite_texture: texture_2d<f32>;

@group(1) @binding(1)
var emissive_texture: texture_2d<f32>;

struct VertexOut {
    @builtin(position) clip_pos: vec4f,
    @location(0) uv: vec2f,
};

fn vs_main(
    vertex_index: u32,
    object_to_world: mat4x4f,
    uv_min: vec2f,
    uv_max: vec2f,
) -> VertexOut {
    let quad_positions = array<vec2f, 6>(
        vec2f(-0.5, -0.5),
        vec2f( 0.5, -0.5),
        vec2f( 0.5,  0.5),
        vec2f(-0.5, -0.5),
        vec2f( 0.5,  0.5),
        vec2f(-0.5,  0.5),
    );

    let quad_uv = array<vec2f, 6>(
        vec2f(0.0, 0.0),
        vec2f(1.0, 0.0),
        vec2f(1.0, 1.0),
        vec2f(0.0, 0.0),
        vec2f(1.0, 1.0),
        vec2f(0.0, 1.0),
    );

    let local = vec4f(quad_positions[vertex_index], 0.0, 1.0);
    let transformed = object_to_world * local;
    let world_pos = transformed.xyz;

    var out: VertexOut;
    let clip_pos = frame.world_to_clip * vec4f(world_pos, 1.0);
    out.clip_pos = clip_pos;
    out.uv = uv_min + quad_uv[vertex_index] * (uv_max - uv_min);
    return out;
}

@vertex
fn vs_entry(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) object_to_world_col0: vec4f,
    @location(1) object_to_world_col1: vec4f,
    @location(2) object_to_world_col2: vec4f,
    @location(3) object_to_world_col3: vec4f,
    @location(4) uv_min: vec2f,
    @location(5) uv_max: vec2f,
) -> VertexOut {
    let object_to_world = mat4x4f(
        object_to_world_col0,
        object_to_world_col1,
        object_to_world_col2,
        object_to_world_col3,
    );
    return vs_main(vertex_index, object_to_world, uv_min, uv_max);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    let color = textureSample(sprite_texture, sprite_sampler, in.uv);

    if color.a <= 0.001 {
        discard;
    }

    let emissive = textureSample(emissive_texture, sprite_sampler, in.uv);

    let light_uv = clamp(in.clip_pos.xy / frame.render_resolution, vec2f(0.0), vec2f(1.0));
    let light_rgb = textureSampleLevel(lighting_texture, sprite_sampler, light_uv, 0.0).rgb;

    let ambient = vec3f(0.02, 0.02, 0.02);
    let lighting = clamp(
        ambient + light_rgb,
        vec3f(0.0),
        vec3f(1.0)
    );

    let lit_color = color.rgb * lighting;
    let final_rgb = lit_color + emissive.rgb;

    return vec4f(final_rgb, color.a);
}
