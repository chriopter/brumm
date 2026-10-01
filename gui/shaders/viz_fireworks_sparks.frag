// Fireworks, the sparks: every live shell drawn where it is now — a rocket
// climbing, or its stars flying out and falling — over the last frame,
// which fades and sags, so each spark leaves a glowing trail. Drawn at half size into a feedback texture
// (qml/viz/Fireworks.qml).
//
// A shell is (x, apex, burst, code): x a part of the width; apex a part of
// the height, with the climb's seconds × 50 as its whole part; burst the
// second it bursts; code a random whole number (kind and colors) plus its
// size as the fraction. Twenty of them come as the rows of five matrices.
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
    float since;   // seconds since the last kick
    float punch;   // how hard that kick was, 0–1
    float kick;    // which kick it was
    float sink;    // how far the trails sink this frame, in texels
    vec4 tint;     // the cover's color, one of the shells' colors
    mat4 m0; mat4 m1; mat4 m2; mat4 m3; mat4 m4;
};
layout(binding = 1) uniform sampler2D prev;

#define ROW(m, r) vec4(m[0][r], m[1][r], m[2][r], m[3][r])

const float GROUND = 0.22; // the waterline, where rockets start, in heights
const float TAU = 6.2831853;
const vec3 GOLD = vec3(1.0, 0.6, 0.2);

float hash(vec3 p) {
    p = fract(p * vec3(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yxz + 33.33);
    return fract((p.x + p.y) * p.z);
}

vec3 palette(float i) {
    if (i < 1.0) return vec3(1.0, 0.12, 0.2);   // red
    if (i < 2.0) return vec3(0.15, 1.0, 0.35);  // green
    if (i < 3.0) return vec3(1.0, 0.7, 0.15);   // gold
    if (i < 4.0) return vec3(0.2, 0.4, 1.0);    // blue
    if (i < 5.0) return vec3(1.0, 0.18, 0.9);   // magenta
    if (i < 6.0) return vec3(0.15, 0.9, 1.0);   // cyan
    return max(tint.rgb * 1.3, vec3(0.15));     // the cover's
}

// How far a star has flown, and how far it has fallen, after a seconds.
float flown(float a, float v, float k) { return v * (1.0 - exp(-k * a)) / k; }
float fallen(float a, float vt, float k) { return vt * (a - (1.0 - exp(-k * a)) / k); }

// A soft spark on the segment a–b, brightest at b.
float spark(vec2 p, vec2 a, vec2 b, float w) {
    vec2 ab = b - a, ap = p - a;
    float h = clamp(dot(ap, ab) / max(dot(ab, ab), 1e-8), 0.0, 1.0);
    vec2 d = ap - ab * h;
    return exp(-dot(d, d) / (w * w)) * (0.35 + 0.65 * h);
}

// A heart, as how far out it reaches in each direction from its cleft.
float heart(float th) {
    float s = sin(th);
    return (2.0 - 2.0 * s + s * sqrt(abs(cos(th))) / (s + 1.4)) * 0.25;
}

vec3 shell(vec2 p, vec4 s) {
    if (s.z <= 0.0) return vec3(0.0);
    float rise = floor(s.y) / 50.0, apex = fract(s.y);
    float a = time - s.z;
    if (a < -rise || a > 4.6) return vec3(0.0);
    float seed = floor(s.w), size = fract(s.w);
    // 0 chrysanthemum, 1 saturn, 2 willow, 3 crossette, 4 heart, 5 pinwheel,
    // 6 a shell of shells, 7 palm
    float kind = mod(seed, 8.0);
    vec3 col = palette(mod(floor(seed / 8.0), 7.0));
    vec3 col2 = palette(mod(floor(seed / 56.0), 7.0));
    vec2 c = vec2(s.x * aspect, apex);
    float w = px * 1.2;
    // each frame draws a little more than its own stretch: no gaps
    float span = max(dt * 1.5, 0.03);

    // Climbing: a hot gold head, leaning in, slowing as it nears the top.
    if (a < 0.0) {
        if (abs(p.x - c.x) > 0.12) return vec3(0.0);
        float lean = (hash(vec3(seed, 1.0, 2.0)) - 0.5) * 0.18;
        float u1 = 1.0 + a / rise, u0 = max(u1 - span / rise, 0.0);
        float e1 = 1.0 - (1.0 - u1) * (1.0 - u1), e0 = 1.0 - (1.0 - u0) * (1.0 - u0);
        vec2 h1 = vec2(c.x + lean * (1.0 - e1) + 0.006 * sin(u1 * 14.0 + seed), mix(GROUND, apex, e1));
        vec2 h0 = vec2(c.x + lean * (1.0 - e0) + 0.006 * sin(u0 * 14.0 + seed), mix(GROUND, apex, e0));
        float flick = 0.6 + 0.4 * hash(vec3(seed, floor(time * 30.0), 1.0));
        return vec3(1.0, 0.72, 0.36) * spark(p, h0, h1, w) * (0.8 + size) * flick;
    }

    bool willow = kind == 2.0;
    float life = (willow ? 3.3 : (kind == 7.0 ? 2.6 : 2.1)) * (0.8 + 0.4 * size);
    if (a > life + 0.8) return vec3(0.0);
    float k = willow ? 2.4 : 1.9;
    float v = 0.3 + 0.42 * size;
    float vt = willow ? 0.18 : 0.1;
    float t0 = max(a - span, 0.0);
    float R = flown(a, v, k), R0 = flown(t0, v, k);
    float fall = fallen(a, vt, 1.4);
    float drop = fall - fallen(t0, vt, 1.4);

    // Seen from the ground: move with the shell's fall, then find the star
    // nearest this pixel's direction in each layer.
    vec2 q = p - c + vec2(0.0, fall);
    float r = length(q);
    if (r > R * 1.3 + 0.09) return vec3(0.0);

    float fade = clamp(1.0 - a / life, 0.0, 1.0);
    fade *= fade;
    float hot = exp(-a * 8.0);             // the first moment: white
    vec3 sum = vec3(0.0);
    for (int j = 0; j < 3; j++) {
        float fj = float(j);
        float n = 16.0, f = 1.0, squash = 1.0, jit = 0.45, bright = 1.0;
        vec3 cj = col;
        if (kind == 0.0) {
            n = j == 0 ? 46.0 : (j == 1 ? 30.0 : 16.0);
            f = j == 0 ? 1.0 : (j == 1 ? 0.72 : 0.4);
            if (j == 2) cj = col2;
        } else if (kind == 1.0) {
            if (j == 2) break;
            if (j == 0) { n = 54.0; squash = 0.3; jit = 0.1; } else { n = 22.0; f = 0.42; cj = col2; }
        } else if (willow) {
            if (j == 2) break;
            n = j == 0 ? 40.0 : 24.0;
            f = j == 0 ? 1.0 : 0.6;
            cj = GOLD;
        } else if (kind == 4.0) {
            if (j == 2) break;
            n = j == 0 ? 70.0 : 44.0;
            f = j == 0 ? 1.0 : 0.55;
            jit = 0.1;
            if (j == 1) cj = col2;
        } else if (kind == 5.0) {
            if (j > 0) break;
            n = 72.0; jit = 0.1;
        } else {
            if (j > 0) break;
            n = kind == 3.0 ? 16.0 : (kind == 6.0 ? 12.0 : 18.0);
            jit = kind == 7.0 ? 0.45 : 0.15;
        }

        // the ring lies tilted; the heart grows from its cleft
        float tilt = (hash(vec3(seed, 3.0, 4.0)) - 0.5) * 1.5;
        float cs = cos(tilt), sn = sin(tilt);
        vec2 ql = q;
        if (squash < 1.0) ql = vec2(cs * q.x + sn * q.y, (cs * q.y - sn * q.x) / squash);
        if (kind == 4.0) ql.y -= 0.5 * R * f;

        float off = fj * 0.37;
        float idx = mod(floor(atan(ql.y, ql.x) / TAU * n - off + 0.5), n);
        float h = hash(vec3(seed, fj, idx));
        float th = (idx + off + (h - 0.5) * jit) / n * TAU;
        float u = hash(vec3(idx, seed, fj + 7.0));
        float sp = 0.95 + 0.05 * u;
        if (kind == 0.0 || willow) sp = j == 0 ? 0.9 + 0.1 * u : f * 1.3 * sqrt(1.0 - u * u);
        else if (kind == 1.0) sp = j == 0 ? 1.0 : f * (0.5 + 0.8 * u);
        else if (kind == 4.0) sp = f * 1.25 * heart(th);
        else if (kind == 5.0) sp = 0.12 + 0.88 * fract(idx / n * 3.0);
        else if (kind == 7.0) sp = 0.7 + 0.3 * u;

        vec2 d = vec2(cos(th), sin(th));
        vec2 b = d * R * sp, a0 = d * R0 * sp;
        if (squash < 1.0) {
            b.y *= squash; a0.y *= squash;
            b = vec2(cs * b.x - sn * b.y, sn * b.x + cs * b.y);
            a0 = vec2(cs * a0.x - sn * a0.y, sn * a0.x + cs * a0.y);
        }
        if (kind == 4.0) { b.y += 0.5 * R * f; a0.y += 0.5 * R0 * f; }

        // crossettes split in four, a shell of shells bursts again
        float t2 = kind == 3.0 ? 0.5 : 0.8;
        if ((kind == 3.0 || kind == 6.0) && a > t2) {
            vec2 bs = d * flown(t2, v, k) * sp;
            float a2 = a - t2;
            float n2 = kind == 3.0 ? 4.0 : 11.0;
            float v2 = kind == 3.0 ? 0.2 : 0.24;
            float r2 = flown(a2, v2, 3.0), r20 = flown(max(a2 - span, 0.0), v2, 3.0);
            vec2 q2 = q - bs;
            float i2 = floor((atan(q2.y, q2.x) - th) / TAU * n2 + 0.5);
            float th2 = i2 / n2 * TAU + th;
            vec2 d2 = vec2(cos(th2), sin(th2));
            b = bs + d2 * r2;
            a0 = bs + d2 * r20;
            cj = kind == 3.0 ? (mod(idx, 2.0) == 0.0 ? col2 : col) : col2;
            bright = 1.5 * exp(-a2 * 1.2) + 2.0 * exp(-a2 * 14.0);
            cj = mix(cj, vec3(1.0), exp(-a2 * 10.0) * 0.6);
        }
        a0.y += drop;

        float b0 = spark(q, a0, b, w * (1.0 + 0.6 * hot) * (kind == 7.0 ? 1.6 : 1.0));
        if (b0 < 0.004) continue;
        // old stars crackle white with the highs
        float tw = 1.0;
        if (a > life * 0.45) {
            float g = hash(vec3(idx + fj * 50.0, seed, floor(time * 22.0)));
            tw = mix(1.0, g > 0.6 ? 2.4 : 0.3, clamp(glitter * 3.0, 0.0, 1.0));
        }
        sum += mix(cj, vec3(1.0), hot * 0.4) * b0 * tw * bright;
    }
    // the brighter a star, the longer its trail
    float heavy = willow ? 1.9 : (kind == 7.0 ? 1.7 : (kind == 0.0 ? 1.4 : 1.15));
    sum *= fade * (0.8 + 0.5 * size) * heavy * (0.6 + 0.4 * smoothstep(0.0, 0.07, R));

    // the burst's own flash, a moment's white core
    sum += vec3(1.0, 0.95, 0.85) * exp(-a * 20.0) * exp(-dot(q, q) / (0.0004 + a * 0.006)) * 2.0;

    // when the stars go out, the highs set a crackle strobing where they were
    float ca = a - life * 0.5;
    if (glitter > 0.03 && ca > 0.0 && r < R) {
        vec2 g = q / 0.013;
        vec2 cell = floor(g);
        float on = hash(vec3(cell, seed + floor(time * 16.0)));
        float thr = 1.0 - 0.14 * clamp(glitter * 3.0, 0.0, 1.0);
        if (on > thr) {
            vec2 f2 = fract(g) - 0.5 - (vec2(hash(vec3(cell, 3.0)), hash(vec3(cell, 9.0))) - 0.5) * 0.6;
            float m = smoothstep(0.0, 0.3, ca) * smoothstep(life + 0.8, life - 0.2, a) * smoothstep(R, R * 0.4, r);
            sum += mix(vec3(1.0, 0.92, 0.75), col, 0.25) * exp(-dot(f2, f2) * 26.0) * m * 1.7;
        }
    }
    return sum;
}

// Mines: every kick throws fans of stars up from the water, at once.
vec3 mines(vec2 p) {
    if (since > 1.3 || p.y > GROUND + 0.5) return vec3(0.0);
    vec3 sum = vec3(0.0);
    float span = max(dt * 1.5, 0.03);
    float t0 = max(since - span, 0.0);
    float v = 0.55 + 0.75 * punch;
    float R = flown(since, v, 3.2), R0 = flown(t0, v, 3.2);
    float fall = fallen(since, 0.3, 1.4);
    float drop = fall - fallen(t0, 0.3, 1.4);
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float x = (0.1 + 0.2 * floor(hash(vec3(kick, fi, 5.0)) * 5.0)) * aspect;
        vec2 q = p - vec2(x, GROUND - fall);
        if (q.y < 0.0 || length(q) > R + 0.03) continue;
        float idx = floor((atan(q.y, q.x) - TAU * 0.25) / 0.085 + 0.5);
        if (abs(idx) > 5.5) continue;
        float h = hash(vec3(idx, kick, fi));
        float th = TAU * 0.25 + (idx + (h - 0.5) * 0.4) * 0.085;
        float sp = 0.55 + 0.45 * hash(vec3(fi, idx, kick));
        vec2 d = vec2(cos(th), sin(th));
        vec3 col = palette(mod(kick + fi * 3.0, 7.0));
        sum += mix(col, vec3(1.0), 0.5 * exp(-since * 9.0))
             * spark(q, d * R0 * sp + vec2(0.0, drop), d * R * sp, px * 1.2) * exp(-since * 2.6) * 1.6;
    }
    return sum;
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = vec2(uv.x * aspect, 1.0 - uv.y);
    // last frame, dimmer, sinking by whole texels (so it stays sharp): the trails
    vec3 old = texture(prev, uv - vec2(0.0, sink * px)).rgb * 2.0;
    old = max(old * exp(-dt * 1.15) - 0.0045, 0.0);
    vec3 c = shell(p, ROW(m0, 0)) + shell(p, ROW(m0, 1)) + shell(p, ROW(m0, 2)) + shell(p, ROW(m0, 3))
           + shell(p, ROW(m1, 0)) + shell(p, ROW(m1, 1)) + shell(p, ROW(m1, 2)) + shell(p, ROW(m1, 3))
           + shell(p, ROW(m2, 0)) + shell(p, ROW(m2, 1)) + shell(p, ROW(m2, 2)) + shell(p, ROW(m2, 3))
           + shell(p, ROW(m3, 0)) + shell(p, ROW(m3, 1)) + shell(p, ROW(m3, 2)) + shell(p, ROW(m3, 3))
           + shell(p, ROW(m4, 0)) + shell(p, ROW(m4, 1)) + shell(p, ROW(m4, 2)) + shell(p, ROW(m4, 3))
           + mines(p);
    // the brighter of the trail and the spark, so nothing piles up to white;
    // stored at half strength: an 8-bit texture keeps light up to 2
    fragColor = vec4(max(old, c) * 0.5, 1.0);
}
