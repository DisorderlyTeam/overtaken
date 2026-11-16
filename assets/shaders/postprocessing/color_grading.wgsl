/*
 * Copyright (c) 2026. DisorderlyTeam
 */

@group(0) @binding(0)
var tex_sampler: sampler;

@group(1) @binding(0)
var texture: texture_2d<f32>;

struct ColorGradingParams {
    exposure: f32,
    contrast: f32,
    saturation: f32,
    brightness: f32,
    tint: vec4f,
};

@group(2) @binding(0)
var<uniform> params: ColorGradingParams;

struct VertexOut {
    @builtin(position) clip_pos: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32
) -> VertexOut {
    var pos = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(3.0, -1.0),
        vec2f(-1.0, 3.0),
    );

    var out: VertexOut;

    out.clip_pos = vec4f(
        pos[vertex_index],
        0.0,
        1.0
    );

    out.uv =
        pos[vertex_index] *
        vec2f(0.5, -0.5) +
        vec2f(0.5, 0.5);

    return out;
}

@fragment
fn fs_main(
    in: VertexOut
) -> @location(0) vec4f {
    let sampled =
        textureSample(
            texture,
            tex_sampler,
            in.uv
        );

    var color = sampled.rgb;

    // Exposure
    color *= params.exposure;

    // Brightness
    color += vec3f(params.brightness);

    // Contrast
    color =
        (color - vec3f(0.5))
        * params.contrast
        + vec3f(0.5);

    // Saturation
    let luminance =
        dot(
            color,
            vec3f(
                0.299,
                0.587,
                0.114
            )
        );

    color =
        mix(
            vec3f(luminance),
            color,
            params.saturation
        );

    // Tint
    color *= params.tint.rgb;

    return vec4f(
        clamp(color, vec3f(0.0), vec3f(1.0)),
        sampled.a
    );
}
