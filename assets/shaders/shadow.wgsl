/*
 * Copyright (c) 2026. DisorderlyTeam
 */

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) quad_0: vec2f,
    @location(1) quad_1: vec2f,
    @location(2) quad_2: vec2f,
    @location(3) quad_3: vec2f,
) -> @builtin(position) vec4f {
    let vertices = array<vec2f, 6>(
        quad_0, quad_2, quad_1,
        quad_1, quad_2, quad_3,
    );

    let vertex = vertices[vertex_index % 6];

    return vec4f(vertex, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return vec4f(0.0, 0.0, 0.0, 1.0);
}
