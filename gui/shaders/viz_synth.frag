// Synth, the screen: the world seen through a lens and a tape. Its colors
// part toward the rim (wider on a kick), its lights bloom from the
// world's mip levels, the sun's rays fall over it, the lens throws a
// streak and ghosts, scanlines lie across it, and on every eighth kick
// the tape slips: bands jump sideways and the colors tear.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;   // seconds
    float aspect; // width / height
    float lines;  // the picture's height in pixels
    float flare;  // the beat, eased in
    float level;
    float glitch; // the tape's slip, 1 fading to 0
    vec2 sun;     // where the sun stands in the picture
    vec4 hot;     // the grid's color
    vec4 ice;     // the ridges' color
};
layout(binding = 1) uniform sampler2D scene;
layout(binding = 2) uniform sampler2D rays;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

// ghost is one reflection inside the lens: a soft disc on the line from
// the sun through the middle.
float ghost(vec2 o, float at, float size) {
    vec2 c = mix(sun, vec2(0.5), at) - (o);
    c.x *= aspect;
    float d = length(c) / size;
    return smoothstep(1.0, 0.75, d) * (0.35 + 0.65 * smoothstep(0.5, 1.0, d));
}

void main() {
    vec2 uv = qt_TexCoord0;

    // the tape slips: some bands jump sideways, one rolls
    float tear = 0.0;
    if (glitch > 0.0) {
        float tick = floor(time * 24.0);
        float band = floor(uv.y * 18.0 + hash(vec2(tick, 1.0)) * 18.0);
        float pick = hash(vec2(band, tick));
        tear = step(0.62, pick) * (hash(vec2(band, tick + 9.0)) - 0.5) * 0.09 * glitch;
        uv.x += tear + sin(uv.y * 90.0 + time * 40.0) * 0.0015 * glitch;
    }

    // the colors part toward the rim
    vec2 c = uv - 0.5;
    float ca = 0.002 + 0.016 * flare + 0.03 * glitch;
    vec2 off = c * dot(c, c) * ca * 4.0 + vec2(tear * 0.5 + 0.004 * glitch, 0.0);
    vec3 col = vec3(texture(scene, uv + off).r, texture(scene, uv).g, texture(scene, uv - off).b);

    // bloom: the world's lights, wide and wider
    vec3 b1 = textureLod(scene, uv, 3.0).rgb;
    vec3 b2 = textureLod(scene, uv, 5.5).rgb;
    col += (b1 * b1 * 0.22 + b2 * b2 * 0.3) * (0.7 + 0.9 * flare + 0.3 * level);

    // the sun's rays through the haze, sparing its own face
    vec2 s = (uv - sun) * vec2(aspect, 1.0);
    col += texture(rays, uv).rgb * vec3(1.0, 0.8, 0.75) * (0.55 + 1.0 * flare) * (0.2 + 0.8 * smoothstep(0.12, 0.3, length(s)));

    // the lens: a streak across the sun and ghosts opposite it
    float streak = exp(-abs(s.y) * 70.0) * exp(-abs(s.x) * 1.6) + 0.5 * exp(-abs(s.y) * 14.0) * exp(-abs(s.x) * 4.0);
    col += mix(ice.rgb, vec3(1.0), 0.35) * streak * (0.3 + 0.7 * flare);
    vec3 gh = ice.rgb * ghost(uv, 1.35, 0.05) + hot.rgb * ghost(uv, 1.7, 0.11)
            + mix(ice.rgb, hot.rgb, 0.5) * ghost(uv, 2.15, 0.035) + vec3(1.0, 0.8, 0.5) * ghost(uv, 0.55, 0.022);
    col += gh * (0.035 + 0.11 * flare);

    // the tape: scanlines, a little grain, noise where it slips
    col *= 1.0 - (0.07 + 0.2 * glitch) * (0.5 + 0.5 * sin(uv.y * lines * 1.5708));
    col += (hash(uv * 733.0 + time) - 0.5) * (0.02 + 0.25 * glitch * step(0.001, abs(tear)));

    // the rim darkens
    col *= 1.0 - 0.75 * dot(c, c);
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0) * qt_Opacity;
}
