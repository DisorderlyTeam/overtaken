/*
 * Copyright (c) 2026. DisorderlyTeam
 */

struct LightUniform {
    color: vec3f,
    intensity: f32,
    center: vec2f,
    radius: f32,
    _pad: f32,
    direction_and_cos_half_angle: vec4f,
};

@group(0) @binding(0) var<uniform> light: LightUniform;
@group(0) @binding(1) var shadow_mask_texture: texture_2d<f32>;

@vertex
fn vs_main(
    @location(0) position: vec2f,
) -> @builtin(position) vec4f {
    return vec4f(position, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let radius = max(light.radius, 1e-5);
    let dist = distance(position.xy, light.center);
    let t = clamp(dist / radius, 0.0, 1.0);
    let base_falloff = (1.0 - t) * (1.0 - t);
    let shaped_falloff = pow(base_falloff, 1.0 / max(light.intensity, 1e-4));
    let direction = light.direction_and_cos_half_angle.xy;
    let cos_half_angle = light.direction_and_cos_half_angle.z;
    let light_alpha = light.direction_and_cos_half_angle.w;
    let to_pixel = select(vec2f(1.0, 0.0), normalize(position.xy - light.center), dist > 1e-5);
    let cone_mask = select(0.0, 1.0, dot(to_pixel, direction) >= cos_half_angle);

    let mask_rgb = textureLoad(shadow_mask_texture, vec2i(position.xy), 0).rgb;
    let radiance = light.color * shaped_falloff * cone_mask * light_alpha * mask_rgb;
    // Alpha is zero so additive light accumulation does not inflate alpha.
    return vec4f(radiance, 0.0);
}
