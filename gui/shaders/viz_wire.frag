// Wire on the screen: the lines burning white at their cores, their
// fading phosphor in the beam's color, a bloom from the phosphor's mip
// levels, a halo in the cover's color behind, and the whole mirrored on a
// glossy floor.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float aspect; // width / height
    float beat;
    float bass;
    float floorY; // where the floor meets the sky, 0 top … 1 bottom
    vec4 ground;  // the dark behind it all
    vec4 deep;    // the beam, dim → lit
    vec4 hot;
    vec4 halo;    // the cover's color
};
layout(binding = 1) uniform sampler2D lines;
layout(binding = 2) uniform sampler2D trail;

// light: the beam's color for an intensity, deep → lit → white
vec3 beam(float e) {
    vec3 c = mix(deep.rgb * 0.7, hot.rgb, smoothstep(0.25, 0.75, e));
    return mix(c, vec3(1.0), smoothstep(0.8, 1.0, e) * 0.85);
}

// glow: the phosphor's light spread wide, from its mip levels
float glow(vec2 uv) {
    return texture(trail, uv, 2.0).r * 0.55 + texture(trail, uv, 4.0).r * 0.9 + texture(trail, uv, 6.0).r * 1.1;
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 c = vec2((uv.x - 0.5) * aspect, uv.y - 0.43);

    // the ground: dark, a halo behind the shape that swells with the bass
    vec3 base = ground.rgb * (1.0 - 0.35 * length(c));
    vec3 col = halo.rgb * exp(-dot(c, c) * (5.0 - 1.5 * bass)) * (0.10 + 0.12 * bass + 0.08 * beat);

    // the lines and their phosphor
    float core = texture(lines, uv).r;
    float tr = texture(trail, uv).r;
    float e = max(core, tr * 0.8);
    col += beam(e) * e * 1.5;
    col += mix(deep.rgb, hot.rgb, 0.2) * glow(uv) * (0.9 + 0.8 * beat);

    // the floor: the shape again, upside down, blurred and fading
    float below = uv.y - floorY;
    if (below > 0.0) {
        vec2 r = vec2(uv.x, floorY - below);
        float fall = exp(-below * 9.0);
        float rt = texture(trail, r, 1.5).r;
        col += (beam(rt) * rt * 0.8 + mix(deep.rgb, hot.rgb, 0.4) * glow(r) * 0.5) * fall * 0.45;
        col *= 0.92;
    }
    // the floor's edge, a thin line of light
    col += mix(deep.rgb, halo.rgb, 0.5) * exp(-abs(below) * 420.0) * 0.18 * exp(-abs(uv.x - 0.5) * 2.2);

    // the light saturates softly instead of clipping
    col = base + 1.0 - exp(-col * 1.2);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
