// Synth, the world: a flight down a wet neon road between hills that are
// the spectrum (highs at the roadside, the bass towering at the flanks),
// toward a striped sun sinking behind a far range. The camera banks,
// dips and bobs; stars drift in two depths, a shooting star crosses on
// the highs. The hills are found by a short march along the ray (28
// steps and 6 more to refine, only where a ray leaves the road).
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;      // seconds
    float aspect;    // width / height
    float run;       // how far the flight has come, wrapped at LAP
    float stripe;    // the sun's stripes sliding down, 0–1
    float flare;     // the beat, eased in
    float bass;
    float high;
    float level;
    float roll;      // the camera's bank, radians
    float dip;       // the horizon's shift, in heights
    float sway;      // the camera's place across the road
    float alt;       // the camera's height over the road
    float shoot;     // the shooting star's way, 0–1; 1 and over: none
    float shootSeed; // where it starts, 0–1
    vec4 hot;        // the grid's color
    vec4 ice;        // the ridges' color
    vec4 tint;       // the cover's color
};
layout(binding = 1) uniform sampler2D spectrum;

const float HY = 0.56;      // the horizon, from the top
const float F = 0.44;       // the lens: heights per unit of slope
const float GAP = 0.62832;  // the grid's rows, a hundred to a lap
const float CELL = 0.8;     // the grid's rails
const float ROAD = 1.6;     // half the road's width
const float FAR = 34.0;     // where the hills end in haze
const int STEPS = 28;

float spec(float x) { return textureLod(spectrum, vec2(clamp(x, 0.0, 1.0) * 0.94 + 0.03, 0.375), 0.0).r; }
float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float tri(float t) { return 1.0 - 2.0 * abs(fract(t) - 0.5); }

// jag is the far range's own rough skyline, 0–1, peaked rather than rounded.
float jag(float x, float k) {
    return 0.5 + 0.24 * tri(x * 2.1 + k) - 0.12 + 0.16 * tri(x * 5.3 + k * 2.3) - 0.08
         + 0.09 * tri(x * 13.7 + k * 4.1) - 0.045 + 0.04 * tri(x * 31.0 + k) - 0.02;
}

// neon lays a glowing line at distance d (pixels) of width w.
float neon(float d, float w) { return smoothstep(w + 1.0, w - 0.5, d) + 0.55 * exp(-d / (w * 3.0 + 2.0)); }

// ground is the land's height at w (across, along): flat on the road,
// rolling hills beside it, each strip raised by its band of the spectrum.
float ground(vec2 w) {
    float a = abs(w.x) - ROAD;
    if (a <= 0.0)
        return 0.0;
    float e = min(a / 7.0, 1.0);
    float s = spec(1.0 - e);
    float shape = 0.58 + 0.42 * sin(w.y * 0.9 + 1.7 * sin(w.x * 0.61)) * sin(w.x * 1.3 + 1.1 * sin(w.y * 0.4));
    return min(a * 0.6, 3.6) * (0.3 + 1.5 * s) * shape;
}

// stars is one depth of the starfield: a star in a few cells of a lattice.
float stars(vec2 g, float px, float twinkle) {
    vec2 cell = floor(g);
    float hs = hash(cell);
    if (hs < 0.95)
        return 0.0;
    vec2 sp = cell + 0.25 + 0.5 * vec2(hash(cell + 7.1), hash(cell + 3.3));
    float tw = 0.5 + 0.5 * sin(time * (0.7 + hs * 12.0) + hs * 60.0);
    float d = length(g - sp) / px * (1.6 - hs * 0.6);
    return mix(tw, 1.0, twinkle) * (exp(-d * d * 0.5) + 0.3 * exp(-d * 0.6));
}

void main() {
    vec2 uv = qt_TexCoord0;
    float px = fwidth(uv.y); // one pixel, in heights
    vec3 violet = mix(vec3(0.35, 0.2, 1.0), hot.rgb, 0.25);
    vec3 col;

    // the camera's view: the picture turned by its bank, the horizon
    // shifted by its dip
    vec2 p = vec2((uv.x - 0.5) * aspect, uv.y - 0.5);
    float cr = cos(roll), sr = sin(roll);
    vec2 q = vec2(cr * p.x - sr * p.y, sr * p.x + cr * p.y);
    float hy = HY - 0.5 + dip;
    float dy = q.y - hy;     // below the horizon, in heights
    float top = hy + 0.5;    // the sky's height

    // the ray, per unit of depth
    vec3 rd = vec3(q.x / F, -dy / F, 1.0);
    float zp = dy > 1e-4 ? alt * F / dy : 1e9; // where it meets the road's plane
    float zEnd = min(zp, FAR);
    float sx = abs(rd.x);
    float z0 = sx > 1e-4 ? (ROAD - sign(rd.x) * sway) / sx : 1e9; // where it leaves the road

    // the march: over the hills only, steps growing with depth
    float z = zp;
    bool hit = dy > 1e-4;
    if (z0 < zEnd && rd.y < 0.75) {
        float zs = max(z0, 0.2), za = zs;
        hit = false;
        for (int i = 1; i <= STEPS; i++) {
            float k = float(i) / float(STEPS);
            float zb = zs + (zEnd - zs) * k * k;
            if (alt + rd.y * zb < ground(vec2(sway + rd.x * zb, zb + run))) {
                for (int j = 0; j < 4; j++) {
                    float zm = 0.5 * (za + zb);
                    if (alt + rd.y * zm < ground(vec2(sway + rd.x * zm, zm + run)))
                        zb = zm;
                    else
                        za = zm;
                }
                // and a straight cut between the last two, so the hit is smooth
                float da = alt + rd.y * za - ground(vec2(sway + rd.x * za, za + run));
                float db = alt + rd.y * zb - ground(vec2(sway + rd.x * zb, zb + run));
                zb = mix(za, zb, clamp(da / max(da - db, 1e-5), 0.0, 1.0));
                z = zb;
                hit = true;
                break;
            }
            za = zb;
        }
        if (!hit && dy > 1e-4) {
            z = zp;
            hit = true;
        }
    }

    // the land's own coordinates, taken where every pixel runs them so
    // their derivatives hold
    vec2 w = vec2(sway + rd.x * z, z + run);
    float gx = w.x / CELL, gz = w.y / GAP;
    float wx = fwidth(gx), wz = fwidth(gz);

    // the sun: its size and place
    float R = min(HY * 0.62, aspect * 0.2) * (1.0 + 0.05 * bass + 0.1 * flare);
    vec2 sc = vec2(0.0, hy - R * 0.35);
    vec3 haze = mix(hot.rgb, vec3(1.0, 0.45, 0.55), 0.4);

    if (!hit) {
        // sky: deep night overhead, burning toward the horizon
        float h = clamp((q.y + 0.5) / top, 0.0, 1.0);
        vec3 night = mix(vec3(0.02, 0.0, 0.07), tint.rgb * 0.18, 0.35);
        col = mix(night, mix(vec3(0.2, 0.02, 0.32), violet * 0.4, 0.4), smoothstep(0.1, 0.75, h));
        col = mix(col, haze * vec3(0.95, 0.5, 0.75), pow(h, 5.0) * 0.8);
        // a slow veil of color high up
        float veil = sin(q.x * 2.3 + time * 0.07 + 2.0 * sin(q.y * 3.1 - time * 0.05)) * sin(q.y * 5.0 + q.x * 1.2);
        col += mix(ice.rgb, hot.rgb, 0.5 + 0.5 * sin(q.x * 1.7 + time * 0.1)) * 0.07 * max(veil, 0.0) * (1.0 - h) * (0.6 + level);

        // stars in two depths, drifting apart as the camera sways
        float fade = smoothstep(1.0, 0.4, h);
        vec2 sq = vec2(q.x, q.y + 0.5);
        col += vec3(0.8, 0.85, 1.0) * fade * stars((sq + vec2(sway * 0.02 + time * 0.002, 0.0)) * 60.0, px * 60.0, high * 0.8);
        col += vec3(1.0, 0.8, 0.95) * fade * 0.7 * stars((sq + vec2(sway * 0.008 + time * 0.0008, 3.0)) * 115.0, px * 115.0, high);

        // the shooting star: a head and its tail, falling aslant
        if (shoot < 1.0) {
            float side = shootSeed < 0.5 ? 1.0 : -1.0;
            vec2 dir = normalize(vec2(side, 0.32));
            vec2 a = vec2(-side * aspect * (0.15 + 0.3 * fract(shootSeed * 7.3)), -0.45 + 0.2 * fract(shootSeed * 3.1));
            vec2 head = a + dir * shoot * 0.9;
            float along = clamp(dot(q - head, -dir), 0.0, 0.22);
            float d = length(q - head + dir * along) / px;
            float s = exp(-d * d * 0.4) * (1.0 - along / 0.22) + 2.0 * exp(-length(q - head) / px * 0.35);
            col += vec3(0.85, 0.95, 1.0) * s * sin(3.1416 * shoot);
        }

        // the air shimmers over the horizon
        float xs = q.x + sin(q.y * 230.0 + time * 6.0) * 0.0016 * (1.0 + 2.0 * level) * smoothstep(0.14, 0.0, -dy);
        float r = length(vec2(xs, q.y) - sc);

        // the sun: yellow to the grid's color, striped in its lower half
        float su = (q.y - (sc.y - R)) / (2.0 * R);
        vec3 sun = mix(vec3(1.0, 0.97, 0.55), vec3(1.0, 0.6, 0.15), smoothstep(0.05, 0.35, su));
        sun = mix(sun, mix(vec3(1.0, 0.15, 0.5), hot.rgb, 0.6), smoothstep(0.3, 0.65, su));
        float body = smoothstep(R + px, R - px, r);
        if (su > 0.3) {
            float f = fract(su * 14.0 - stripe);
            float gap = (su - 0.3) * 2.0;
            float e = px * 14.0 / (2.0 * R);
            body *= smoothstep(gap - e, gap + e, f);
        }
        // gloss across its crown, its light spilling into the sky, and a
        // crown of spikes thrown out by a kick
        sun += vec3(0.5, 0.45, 0.35) * smoothstep(0.3, 0.0, su) * smoothstep(0.7 * R, 0.1 * R, abs(xs - sc.x));
        float halo = max(r - R, 0.0);
        vec3 glow = mix(vec3(1.0, 0.3, 0.5), hot.rgb, 0.5);
        col += glow * (0.5 * exp(-halo * 14.0) + 0.2 * exp(-halo * 3.5)) * (1.0 + 1.2 * flare);
        float ang = atan(q.y - sc.y, xs);
        float spikes = pow(abs(sin(ang * 9.0 + time * 0.15)), 12.0) + 0.6 * pow(abs(sin(ang * 23.0 - time * 0.1)), 20.0);
        col += vec3(1.0, 0.7, 0.5) * spikes * exp(-halo * 6.0) * (0.08 + 0.6 * flare) * smoothstep(0.0, 0.02, halo);
        col = mix(col, sun * (0.92 + 0.2 * flare), body);

        // the far range: violet glass with a lit ridge, the mids in it
        float u = abs(xs) / (aspect * 0.5);
        float far = 0.42 * top * clamp((u - 0.16) / 0.6, 0.0, 1.0)
                  * (0.2 + 0.45 * jag(xs + 3.7, 1.3) + 0.6 * spec(0.25 + 0.6 * u));
        float df = (dy + far) / px;
        if (df > 0.0) {
            float rel = -dy / max(far, px);
            vec3 m = mix(vec3(0.05, 0.01, 0.12), violet * 0.3, rel);
            m += violet * 0.4 * exp(-df * 0.05) + violet * 0.3 * smoothstep(0.85, 1.0, abs(fract(rel * 5.0) - 0.5) * 2.0) * rel;
            col = mix(m, haze * 0.5, 0.25);
        }
        col += mix(violet, vec3(1.0), 0.35) * 1.5 * neon(abs(df), 1.0) * step(1.5 * px, far);
    } else {
        float h = ground(w);
        float a = abs(w.x) - ROAD;
        float v = 1.0 / max(z, 1.0);
        float fog = exp(-z * 0.075);
        float thick = 0.3 + 1.5 * v;
        float lx = neon(abs(fract(gx + 0.5) - 0.5) / wx, thick);
        float lz = neon(abs(fract(gz + 0.5) - 0.5) / wz, thick);
        // far off the lines crowd: fade to their average glow
        lx = mix(lx, 0.25, smoothstep(0.15, 0.5, wx));
        lz = mix(lz, 0.25, smoothstep(0.15, 0.5, wz));
        float grid = max(lx, lz);

        if (a <= 0.0) {
            // the road: a wet dark mirror
            col = mix(vec3(0.1, 0.01, 0.2), vec3(0.01, 0.0, 0.04), smoothstep(0.0, 0.7, v));

            // the sun mirrored: a trembling column of light under it
            float wob = sin(z * 9.0 - time * 3.0) * 0.012 * v + sin(z * 23.0 + time * 5.0) * 0.004;
            float cw = R * (0.5 + 0.7 * v);
            float column = exp(-pow(abs(q.x + wob) / cw, 2.0) * 3.0) * exp(-v * 1.8);
            // the stripes ride down the mirror too
            column *= 0.75 + 0.25 * sin(z * 3.0 + stripe * 6.2832);
            col += mix(vec3(1.0, 0.6, 0.2), mix(vec3(1.0, 0.1, 0.5), hot.rgb, 0.5), smoothstep(0.0, 0.4, v)) * column * (1.5 + flare);

            vec3 gc = mix(violet * 0.8, hot.rgb, smoothstep(7.0, 1.5, z));
            gc = mix(gc, vec3(1.0, 0.9, 1.0), flare * smoothstep(4.0, 1.0, z) * 0.8);
            col += gc * grid * (1.1 + 0.9 * flare + 0.4 * level) * smoothstep(0.0, 0.06, v);
            // the kerbs: two rails of light the hills rise from
            float kerb = abs(a) / max(fwidth(w.x), 1e-5);
            col += mix(ice.rgb, vec3(1.0), 0.4) * neon(kerb, 1.0 + 2.0 * v) * (0.5 + 0.3 * fog) * (1.0 + flare);
        } else {
            // a hill: dark glass under the grid, lit by its band, its
            // crest burning where it turns away from the eye
            float e = 0.06;
            vec3 n = normalize(vec3(h - ground(w + vec2(e, 0.0)), e, h - ground(w + vec2(0.0, e))));
            float edge = pow(1.0 - abs(dot(n, normalize(rd))), 6.0);
            float s = spec(1.0 - min(a / 7.0, 1.0));
            float up = smoothstep(0.0, 1.8, h);
            vec3 lit = mix(hot.rgb, ice.rgb, up);
            col = mix(vec3(0.02, 0.0, 0.06), lit * 0.16, up) * (0.6 + 0.8 * s);
            col += lit * grid * (0.7 + 1.6 * s + 0.6 * flare);
            col += mix(lit, vec3(1.0), 0.5) * edge * (1.2 + 1.5 * s);
            col += mix(ice.rgb, vec3(1.0), 0.5) * smoothstep(1.3, 2.6, h) * 0.3;
            // the kerb again, seen from the hill's side
            col += mix(ice.rgb, vec3(1.0), 0.4) * exp(-a * 9.0) * 0.6;
        }
        col = mix(haze * 0.55, col, fog);
    }

    // the horizon: a white-hot line with a haze of color over both sides,
    // behind the hills
    if (!hit || abs(w.x) < ROAD) {
        float dh = abs(dy) / px;
        col += mix(haze, vec3(1.0), 0.4) * (smoothstep(2.0, 0.0, dh) + 0.35 * exp(-dh * 0.08)) * (1.0 + flare);
        col += hot.rgb * 0.22 * exp(-abs(dy) * 9.0) * (0.6 + 0.6 * level);
    }

    // hot spots burn to white
    col += max(col - 1.0, 0.0).gbr * 0.4;
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
