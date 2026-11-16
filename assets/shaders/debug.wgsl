/*
 * Copyright (c) 2026. DisorderlyTeam
 */

struct DebugUniform {
    world_to_clip: mat4x4f,
};

@group(0) @binding(0)
var<uniform> debug_uniform: DebugUniform;

struct VertexOut {
    @builtin(position) clip_pos: vec4f,
    @location(0) color: vec4f,
};

@vertex
fn vs_main(
    @location(0) position: vec4f,
    @location(1) color: vec4f,
) -> VertexOut {
    var out: VertexOut;
    out.clip_pos = debug_uniform.world_to_clip * position;
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    return in.color;
}
