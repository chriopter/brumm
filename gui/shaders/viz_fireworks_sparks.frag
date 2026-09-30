// Fireworks, the sparks: every live shell drawn where it is now — a rocket
// climbing, or its stars flying out and falling — over the last frame,
// faded and sagging a little, so each spark leaves a trail. Drawn at half
// size into a feedback texture (qml/viz/Fireworks.qml).
//
// A shell is (x, apex, launched, code): x and apex as parts of the width
// and height, launched in seconds, code a random whole number (kind and
// color) plus its size as the fraction.
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
    float glitter; // the highs: old stars crackle
    vec4 tint;     // the cover's color, one of the shells' colors
    vec4 s0; vec4 s1; vec4 s2; vec4 s3; vec4 s4; vec4 s5;
    vec4 s6; vec4 s7; vec4 s8; vec4 s9; vec4 s10; vec4 s11;
};
layout(binding = 1) uniform sampler2D prev;

const float RISE = 0.85;   // seconds from the ground to the burst
const float GROUND = 0.2;  // where rockets start, in heights
const float TAU = 6.2831853;

float hash(vec3 p) {
    p = fract(p * vec3(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yxz + 33.33);
    return fract((p.x + p.y) * p.z);
}

vec3 palette(float i) {
    if (i < 1.0) return vec3(1.0, 0.22, 0.28);  // red
    if (i < 2.0) return vec3(0.25, 1.0, 0.42);  // green
    if (i < 3.0) return vec3(1.0, 0.78, 0.25);  // gold
    if (i < 4.0) return vec3(0.3, 0.48, 1.0);   // blue
    if (i < 5.0) return vec3(1.0, 0.28, 0.92);  // magenta
    if (i < 6.0) return vec3(0.25, 0.95, 1.0);  // cyan
    return max(tint.rgb * 1.3, vec3(0.15));     // the cover's
}

// How far a star has flown, and how far it has fallen, after a seconds.
float flown(float a, float v, float k) { return v * (1.0 - exp(-k * a)) / k; }
float fallen(float a, float vt, float k) { return vt * (a - (1.0 - exp(-k * a)) / k); }

// A soft spark on the segment a–b.
float spark(vec2 p, vec2 a, vec2 b, float w) {
    vec2 ab = b - a, ap = p - a;
    float h = clamp(dot(ap, ab) / max(dot(ab, ab), 1e-8), 0.0, 1.0);
    vec2 d = ap - ab * h;
    return exp(-dot(d, d) / (w * w)) * (0.35 + 0.65 * h);
}

vec3 shell(vec2 p, vec4 s) {
    float tau = time - s.z;
    if (s.z <= 0.0 || tau < 0.0 || tau > RISE + 3.6) return vec3(0.0);
    float seed = floor(s.w), size = fract(s.w);
    float kind = mod(seed, 4.0);           // 0 peony, 1 ring, 2 willow, 3 crossette
    vec3 col = palette(mod(floor(seed / 4.0), 7.0));
    vec3 col2 = palette(mod(floor(seed / 28.0), 7.0));
    vec2 base = vec2(s.x * aspect, GROUND);
    float apex = s.y;
    float w = px * 1.15;
    // each frame draws a little more than its own stretch: no gaps
    float span = max(dt * 1.5, 0.05);

    // Climbing: a hot gold head, slowing as it nears the top.
    if (tau < RISE) {
        vec2 h1 = vec2(base.x + 0.012 * sin(tau * 9.0 + seed), mix(GROUND, apex, 1.0 - pow(1.0 - tau / RISE, 2.0)));
        float t0 = max(tau - span, 0.0);
        vec2 h0 = vec2(base.x + 0.012 * sin(t0 * 9.0 + seed), mix(GROUND, apex, 1.0 - pow(1.0 - t0 / RISE, 2.0)));
        float flick = 0.7 + 0.3 * hash(vec3(seed, floor(time * 30.0), 1.0));
        return vec3(1.0, 0.75, 0.4) * spark(p, h0, h1, w) * 1.6 * flick;
    }

    float a = tau - RISE;
    bool willow = kind == 2.0;
    float life = (willow ? 3.4 : 2.0) * (0.85 + 0.3 * size);
    if (a > life) return vec3(0.0);
    float k = willow ? 2.4 : 1.7;
    float v = (0.28 + 0.2 * size) * (willow ? 0.95 : 1.0);
    float vt = willow ? 0.16 : 0.11;
    float R = flown(a, v, k), R0 = flown(max(a - span, 0.0), v, k);
    float drop = fallen(a, vt, 1.4) - fallen(max(a - span, 0.0), vt, 1.4);

    // Seen from the ground: move with the shell's fall, then find the star
    // nearest this pixel's direction on each shell of the sphere.
    vec2 c = vec2(base.x, apex);
    vec2 q = p - c + vec2(0.0, fallen(a, vt, 1.4));
    float r = length(q);
    if (r > R * 1.12 + 6.0 * w) return vec3(0.0);
    float ang = atan(q.y, q.x);

    float fade = 1.0 - a / life;
    fade *= fade;
    float hot = exp(-a * 8.0);             // the first moment: white
    vec3 sum = vec3(0.0);
    int layers = kind == 1.0 ? 1 : (willow ? 2 : 3);
    for (int j = 0; j < 3; j++) {
        if (j >= layers) break;
        float fj = float(j);
        float n = kind == 1.0 ? 44.0 : (fj == 0.0 ? 34.0 : (fj == 1.0 ? 26.0 : 16.0));
        float f = kind == 1.0 ? 1.0 : (fj == 0.0 ? 0.97 : (fj == 1.0 ? 0.74 : 0.45));
        float off = fj * 0.37;
        float idx = floor(ang / TAU * n - off + 0.5);
        float h = hash(vec3(seed, fj, idx));
        float th = (idx + off + (h - 0.5) * 0.45) / n * TAU;
        float u = hash(vec3(idx, seed, fj + 7.0));
        float sp = kind == 1.0 ? 0.96 + 0.08 * u : (fj == 0.0 ? 0.92 + 0.1 * u : f * 1.3 * sqrt(1.0 - u * u));
        vec2 d = vec2(cos(th), sin(th));
        vec2 b = d * R * sp, a0 = d * R0 * sp + vec2(0.0, drop);
        float b0 = spark(q, a0, b, w * (1.0 + 0.5 * hot));
        if (b0 < 0.004) continue;
        vec3 c0 = kind == 3.0 && mod(idx, 2.0) == 0.0 ? col2 : col;
        if (willow) c0 = vec3(1.0, 0.62, 0.22);
        // old stars crackle white with the highs
        float tw = 1.0;
        if (a > life * 0.45) {
            float g = hash(vec3(idx + fj * 50.0, seed, floor(time * 22.0)));
            tw = mix(1.0, g > 0.6 ? 2.2 : 0.35, clamp(glitter * 3.0, 0.0, 1.0) * (willow ? 1.0 : 0.8));
        }
        sum += mix(c0, vec3(1.0), hot * 0.35) * b0 * tw;
    }
    // the burst's own flash, a moment's white core
    sum += vec3(1.0, 0.95, 0.85) * exp(-a * 22.0) * exp(-dot(q, q) / (0.0003 + a * 0.004)) * 1.4;
    return sum * fade * (0.9 + 0.5 * size) * (0.35 + 0.65 * smoothstep(0.0, 0.07, R));
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = vec2(uv.x * aspect, 1.0 - uv.y);
    // last frame, dimmer and sinking a touch: the trails
    vec3 old = texture(prev, uv - vec2(0.0, dt * 0.018)).rgb * 2.0;
    vec3 c = max(old * exp(-dt * 2.6) - 0.012, 0.0);
    c += shell(p, s0) + shell(p, s1) + shell(p, s2) + shell(p, s3)
       + shell(p, s4) + shell(p, s5) + shell(p, s6) + shell(p, s7)
       + shell(p, s8) + shell(p, s9) + shell(p, s10) + shell(p, s11);
    // stored at half strength: an 8-bit texture keeps light up to 2
    fragColor = vec4(c * 0.5, 1.0);
}
