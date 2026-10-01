// Fire, the burning: heat, smoke and embers kept from frame to frame at
// half size (qml/viz/Fire.qml). Last frame is carried up by a rising,
// curling wind and cools on the way; new heat comes from the flames on the
// floor, each column as high as its part of the spectrum, and from the
// fireballs the kicks throw up. Where flames cool, smoke is left; embers
// fly up in waves, one per kick, and leave streaks.
//
// Red is heat, green smoke, blue the embers.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;    // seconds
    float dt;      // since the last frame
    float aspect;  // width / height
    float px;      // a pixel of this texture, in heights
    float reach;   // how high the flames on the floor reach, in heights
    float rise1;   // how far each octave of noise has risen
    float rise2;
    float flare;   // a kick, eased
    float level;   // loudness
    float embers;  // how many embers fly without a kick
    vec4 ages;     // the last four kicks: seconds since each
    vec4 ballx;    // where each threw its fireball, as part of the width
    vec4 ballpow;  // and how strong it was
};
layout(binding = 1) uniform sampler2D prev;
layout(binding = 2) uniform sampler2D spectrum;

const float FLOOR = 0.2; // the floor's edge, in heights; below it nothing burns

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

// value noise, repeating every 256 upward so the rise can wrap there
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    float y0 = mod(i.y, 256.0), y1 = mod(i.y + 1.0, 256.0);
    return mix(mix(hash(vec2(i.x, y0)), hash(vec2(i.x + 1.0, y0)), u.x),
               mix(hash(vec2(i.x, y1)), hash(vec2(i.x + 1.0, y1)), u.x), u.y);
}

// How many embers were thrown up tau seconds ago: a wave after each kick.
float thrown(float tau, float jitter) {
    vec4 e = ages - tau + jitter;
    vec4 w = step(0.0, e) * exp(-e * 4.5) * ballpow;
    return embers + 1.6 * (w.x + w.y + w.z + w.w);
}

// One layer of embers: one in some cells of a grid rising at speed v,
// swaying wider the higher it is, drawn as long as it flew this frame.
float emberLayer(vec2 p, float cell, float v, float seed) {
    vec2 q = vec2(p.x, p.y - time * v) / cell;
    vec2 id = floor(q);
    float r = hash(id + seed), r2 = hash(id + seed + 3.1);
    float tau = p.y / v;
    float life = 0.5 + 1.1 * r2;
    if (tau > life || r > thrown(tau, (r2 - 0.5) * 0.25) * 0.26) return 0.0;
    vec2 at = vec2(0.3 + 0.4 * r2, 0.2 + 0.6 * hash(id + seed + 7.7));
    float turn = time * (1.5 + 3.0 * r2) + r * 40.0, sway = 0.3 * min(1.0, p.y * 3.0);
    at.x += sway * sin(turn);
    vec2 d = (fract(q) - at) * cell;
    d.x -= d.y * sway * cos(turn) * (1.5 + 3.0 * r2) * cell / v; // the streak leans the way it flies
    float size = px * (0.45 + 0.5 * r2);
    d.y = max(abs(d.y) - v * dt * 0.8, 0.0) * 0.5; // as long as it flew, and a soft end: no gaps
    float fade = 1.0 - tau / life;
    return exp(-dot(d, d) / (size * size)) * fade * (0.5 + r2);
}

void main() {
    vec2 uv = qt_TexCoord0;
    float h = 1.0 - uv.y - FLOOR; // height above the floor
    if (h < -0.01) { fragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }
    float x = uv.x * aspect;

    // the wind: up, faster with the music, and curling — the slope of one
    // noise turned a quarter, so it swirls without piling up
    vec2 np = vec2(x * 3.2, h * 3.2 - rise1 * 0.55);
    float n0 = noise(np);
    vec2 curl = vec2(noise(np + vec2(0.0, 0.35)) - n0, n0 - noise(np + vec2(0.35, 0.0)));
    float up = (0.32 + 0.45 * level + 0.55 * flare) * (0.65 + 0.7 * n0);
    vec2 wind = vec2(0.0, up) + curl * (0.5 + 2.2 * h) * (0.6 + level);
    vec4 old = texture(prev, uv + vec2(-wind.x / aspect, wind.y) * dt);

    // heat cools as it climbs; an 8-bit texture needs a whole step a frame
    float heat = max(old.r * exp(-dt * (1.7 - 0.9 * level)) - max(dt * 0.2, 0.0045), 0.0);

    // the flames on the floor: the bass in the middle, the highs outside,
    // calm at the base and torn into tongues toward the tips
    float e = texture(spectrum, vec2((abs(uv.x - 0.5) * 2.0 * 63.0 + 0.5) / 64.0, 1.5 / 4.0)).r;
    float r = max(0.02, reach * (0.25 + 1.15 * e));
    float rel = h / r;
    if (rel < 1.4) {
        float n = 0.6 * noise(vec2((x + curl.x * 0.25) * 10.0, h * 3.5 - rise1))
                + 0.4 * noise(vec2((x - curl.y * 0.2) * 24.0 + 61.0, h * 8.0 - rise2));
        float src = (1.0 - rel) * (0.62 + 0.4 * e) + (n - 0.5) * (0.7 + 2.6 * min(rel * rel, 1.0));
        heat = max(heat, min(src, 1.2 - rel));
    }

    // the fireballs: each kick shoots one up on a jet; the wind rolls it over
    for (int i = 0; i < 4; i++) {
        float a = ages[i], pw = ballpow[i];
        if (a > 1.7 || pw <= 0.0) continue;
        float cy = 0.78 * a * (1.0 - 0.28 * a);
        vec2 dd = vec2(x - ballx[i] * aspect, h - cy);
        float rad = (0.04 + 0.07 * a) * (0.6 + 0.6 * pw);
        float ball = exp(-dot(dd, dd) / (rad * rad)) * exp(-a * 2.0) * (0.9 + 0.6 * pw);
        float jw = 0.014 * (0.6 + pw) * (1.0 + 2.0 * (cy - h));
        float jet = exp(-dd.x * dd.x / (jw * jw)) * step(h, cy) * exp(-a * 5.0) * 1.2;
        heat = max(heat, (ball + jet) * (0.7 + 0.6 * n0));
    }
    heat = min(heat, 1.0);

    // smoke: left where the flames' edges cool, carried on, thinning slowly
    float smoke = max(old.g * exp(-dt * 0.35) - max(dt * 0.1, 0.004), 0.0);
    smoke += dt * 2.6 * smoothstep(0.06, 0.25, heat) * smoothstep(0.6, 0.3, heat) * smoothstep(0.05, 0.2, h);
    smoke = min(smoke, 1.0);

    // embers over their own fading last frame: streaks
    float trail = texture(prev, uv + vec2(-curl.x * 0.6 / aspect, 0.0) * dt).b;
    float ember = max(trail * exp(-dt * 13.0) - 0.006, 0.0);
    vec2 p = vec2(x, max(h, 0.0));
    ember += emberLayer(p, 0.04, 0.5, 0.0) + emberLayer(p + vec2(0.31, 0.0), 0.0625, 0.75, 17.0);

    fragColor = vec4(heat, smoke, min(ember, 1.0), 1.0);
}
