// Fire: flames over the whole floor (qml/viz/Fire.qml). Each column burns
// as high as its part of the spectrum, the bass in the middle; two octaves
// of noise rising at different speeds cut the heat into tongues that lick,
// split and break off, calm at the base and torn toward the tips. The heat
// glows through crimson, orange and gold to white, blooms into the dark
// above, and sparks fly up on a kick.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;   // seconds, standing still when the fire does
    float aspect; // width / height
    float reach;  // how high the flames reach, in heights
    float rise1;  // how far each octave of noise has risen
    float rise2;
    float sparks; // how much flies up: 0 still, 1+ on a kick
    float flare;  // a kick, eased
    vec4 tint;    // the cover's color
};
layout(binding = 1) uniform sampler2D spectrum;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

// value noise, repeating every 256 upward so the rise can wrap there
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    float y0 = mod(i.y, 256.0), y1 = mod(i.y + 1.0, 256.0);
    return mix(mix(hash(vec2(i.x, y0)), hash(vec2(i.x + 1.0, y0)), u.x),
               mix(hash(vec2(i.x, y1)), hash(vec2(i.x + 1.0, y1)), u.x), u.y);
}

// the glow of heat h: a red haze outside, then crimson, orange, gold, white
vec3 glow(float h) {
    return vec3(0.3, 0.02, 0.03) * smoothstep(-0.15, 0.05, h) // the tongues' red haze
         + vec3(0.45, 0.03, 0.0) * smoothstep(0.0, 0.25, h)
         + vec3(0.45, 0.22, 0.0) * smoothstep(0.2, 0.55, h)
         + vec3(0.1, 0.45, 0.05) * smoothstep(0.5, 0.85, h)
         + vec3(0.5, 0.55, 0.8) * smoothstep(0.8, 1.15, h);
}

// one layer of sparks: a spark in some cells of a grid rising at speed v
float sparkLayer(vec2 p, float cell, float v, float seed) {
    vec2 q = vec2(p.x, p.y - time * v) / cell;
    vec2 id = floor(q);
    float r = hash(id + seed);
    if (r > sparks * 0.45) return 0.0;
    vec2 at = vec2(0.2 + 0.6 * hash(id + seed + 3.1), 0.2 + 0.6 * hash(id + seed + 7.7));
    at.x += 0.15 * sin(time * 3.0 + r * 40.0);
    vec2 d = (fract(q) - at) * cell * vec2(1.0, 0.4); // drawn out as they fly
    float size = cell * (0.02 + 0.025 * hash(id + seed + 1.3));
    return exp(-dot(d, d) / (size * size)) * (1.0 - r / (sparks * 0.45)); // fades in and out as sparks does
}

void main() {
    vec2 uv = qt_TexCoord0;
    float x = uv.x * aspect;
    float d = 1.0 - uv.y; // height above the floor

    // how high this column burns: the bass in the middle, the highs outside
    float e = texture(spectrum, vec2((abs(uv.x - 0.5) * 2.0 * 63.0 + 0.5) / 64.0, 1.5 / 4.0)).r;
    float r = max(0.02, reach * (0.45 + 0.75 * sqrt(e)));
    float rel = d / r; // 0 at the base, 1 at the flame's reach

    // calm at the base, torn into tongues toward the tips
    vec3 col = vec3(0.0);
    if (rel < 1.6) {
        float warp = noise(vec2(x * 3.0, d * 2.0 - rise1)) - 0.5;
        float n = 0.6 * noise(vec2((x + warp * 0.15) * 10.0, d * 3.5 - rise1))
                + 0.4 * noise(vec2((x - warp * 0.1) * 24.0 + 61.0, d * 8.0 - rise2));
        float heat = (1.0 - rel) * 1.05 + (n - 0.5) * (0.6 + 2.8 * min(rel * rel, 1.0));
        heat = min(heat, 1.25 - rel); // tips cool to red, however they tear
        col = glow(heat) * (1.0 + 0.5 * flare);
    }

    // the bloom: the flames' light in the dark over them, and a hot floor
    float bloom = exp(-max(rel - 0.3, 0.0) * 1.3) * (0.55 + 0.45 * sqrt(e));
    col += mix(vec3(0.9, 0.12, 0.03), tint.rgb, 0.3) * bloom * (0.3 + 0.35 * flare);
    col += vec3(1.0, 0.45, 0.1) * exp(-d * 18.0) * (0.35 + 0.4 * flare);

    // sparks: a burst on a kick, a trickle with the loudness, fading as they climb
    vec2 p = vec2(x, d);
    float s = sparkLayer(p, 0.07, 0.45, 0.0) + sparkLayer(p + 0.3, 0.11, 0.3, 17.0) * 0.8;
    col += vec3(1.0, 0.6, 0.25) * s * 2.5 * smoothstep(1.0, 0.1, d) * smoothstep(0.02, 0.15, d);

    // hot spots bloom toward white rather than clip
    col = 1.0 - exp(-col * 1.4);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
