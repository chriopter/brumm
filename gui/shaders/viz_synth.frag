// Synth: a striped sun sinks into the horizon behind two ranges of
// mountains raised by the spectrum (bass at the flanks, the valley kept
// open for the sun), stars twinkle with the highs, and a neon grid races
// toward the viewer over a glossy floor that mirrors the sun. Everything
// glows; a kick swells the sun and flashes the grid white. One pass.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;   // seconds
    float aspect; // width / height
    float run;    // how far the grid has come, wrapped at one row
    float stripe; // the sun's stripes sliding down, 0–1
    float flare;  // the beat, eased in
    float bass;
    float high;
    float level;
    float live;
    vec4 tint;    // the cover's color
};
layout(binding = 1) uniform sampler2D spectrum;

const float HY = 0.56;  // the horizon, from the top
const float GAP = 0.6;  // the grid's row spacing in depth

float spec(float x) { return texture(spectrum, vec2(clamp(x, 0.0, 1.0) * 0.97 + 0.015, 0.375)).r; }
float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float tri(float t) { return 1.0 - 2.0 * abs(fract(t) - 0.5); }

// jag is a range's own rough skyline, 0–1, peaked rather than rounded.
float jag(float x, float k) {
    return 0.5 + 0.24 * tri(x * 2.1 + k) - 0.12 + 0.16 * tri(x * 5.3 + k * 2.3) - 0.08
         + 0.09 * tri(x * 13.7 + k * 4.1) - 0.045 + 0.04 * tri(x * 31.0 + k) - 0.02;
}

// neon lays a glowing line at distance d (pixels) of width w.
float neon(float d, float w) { return smoothstep(w + 1.0, w - 0.5, d) + 0.55 * exp(-d / (w * 3.0 + 2.0)); }

void main() {
    vec2 uv = qt_TexCoord0;
    float px = fwidth(uv.y);         // one pixel, in heights
    float x = (uv.x - 0.5) * aspect; // centered, in heights
    float u = abs(uv.x - 0.5) * 2.0; // 0 middle … 1 edges
    float y = uv.y;
    vec3 hot = vec3(1.0, 0.18, 0.62);  // neon magenta
    vec3 ice = vec3(0.1, 0.85, 1.0);   // neon cyan
    vec3 violet = vec3(0.35, 0.2, 1.0);
    vec3 col;

    // the sun: its size and place, as the terminal's
    float R = min(HY * 0.62, aspect * 0.2) * (1.0 + 0.05 * bass + 0.07 * flare);
    vec2 sc = vec2(0.0, HY - R * 0.35);
    float r = length(vec2(x, y) - sc);

    // the floor's depth and grid coordinates, taken here where every pixel
    // runs them so their derivatives hold
    float v = max((y - HY) / (1.0 - HY), 1e-3);
    float z = 1.0 / v;
    float cam = sin(time * 0.17) * 0.7;
    float gx = x / aspect / v * 8.0 + cam; // across: the rails
    float gz = (z + run) / GAP;            // along: the rows
    float wx = fwidth(gx), wz = fwidth(gz);

    if (y < HY) {
        // sky: deep night overhead, burning toward the horizon
        float h = y / HY;
        vec3 top = mix(vec3(0.02, 0.0, 0.07), tint.rgb * 0.18, 0.35);
        col = mix(top, vec3(0.2, 0.02, 0.32), smoothstep(0.1, 0.75, h));
        col = mix(col, vec3(0.95, 0.2, 0.45), pow(h, 5.0) * 0.8);

        // stars, twinkling with the highs
        vec2 g = vec2(x, y) * 70.0;
        vec2 cell = floor(g);
        float hs = hash(cell);
        if (hs > 0.93) {
            vec2 sp = cell + 0.25 + 0.5 * vec2(hash(cell + 7.1), hash(cell + 3.3));
            float tw = 0.5 + 0.5 * sin(time * (0.7 + hs * 12.0) + hs * 60.0);
            tw = mix(tw, 1.0, high * 0.8) * smoothstep(1.0, 0.45, h);
            float d = length(g - sp) / (px * 70.0) * (1.6 - hs * 0.6);
            col += vec3(0.8, 0.85, 1.0) * tw * (exp(-d * d * 0.5) + 0.3 * exp(-d * 0.6));
        }

        // the sun: yellow to magenta, striped in its lower half
        float su = (y - (sc.y - R)) / (2.0 * R);
        vec3 sun = mix(vec3(1.0, 0.97, 0.55), vec3(1.0, 0.6, 0.15), smoothstep(0.05, 0.35, su));
        sun = mix(sun, vec3(1.0, 0.15, 0.5), smoothstep(0.3, 0.65, su));
        float body = smoothstep(R + px, R - px, r);
        if (su > 0.3) {
            float f = fract(su * 14.0 - stripe);
            float gap = (su - 0.3) * 2.0;
            float e = px * 14.0 / (2.0 * R);
            body *= smoothstep(gap - e, gap + e, f);
        }
        // gloss across its crown, and its light spilling into the sky
        sun += vec3(0.5, 0.45, 0.35) * smoothstep(0.3, 0.0, su) * smoothstep(0.7 * R, 0.1 * R, abs(x - sc.x));
        float halo = max(r - R, 0.0);
        col += vec3(1.0, 0.3, 0.5) * (0.6 * exp(-halo * 14.0) + 0.22 * exp(-halo * 3.5)) * (1.0 + 0.8 * flare);
        col = mix(col, sun, body);

        // two ranges: the far one in violet with the highs, the near one
        // in cyan with the bass, each hiding what lies behind it
        float far = 0.8 * HY * clamp((u - 0.2) / 0.6, 0.0, 1.0)
                  * (0.2 + 0.45 * jag(x + 3.7, 1.3) + 0.5 * spec(0.3 + 0.7 * u));
        float e = clamp((u - 0.12) / 0.75, 0.0, 1.0);
        e = e * e * (3.0 - 2.0 * e);
        float near = 0.56 * HY * e * (0.2 + 0.45 * jag(x, 0.0) + 0.6 * spec((1.0 - u) * 0.9));

        float ft = HY - far, nt = HY - near;
        float df = (y - ft) / px, dn = (y - nt) / px;
        if (df > 0.0) {
            // the far range: dark glass, its strata lit from the rim down
            float rel = (HY - y) / max(far, px);
            vec3 m = mix(vec3(0.03, 0.01, 0.1), vec3(0.12, 0.03, 0.25), rel);
            m += violet * 0.35 * exp(-df * 0.05) + violet * 0.25 * smoothstep(0.85, 1.0, abs(fract(rel * 5.0) - 0.5) * 2.0) * rel;
            col = m;
        }
        col += vec3(0.6, 0.4, 1.0) * 1.5 * neon(abs(df), 1.0) * step(1.5 * px, far);
        if (dn > 0.0) {
            float rel = (HY - y) / max(near, px);
            vec3 m = mix(vec3(0.01, 0.0, 0.05), vec3(0.03, 0.08, 0.2), rel);
            m += ice * 0.3 * exp(-dn * 0.04);
            m += ice * 0.3 * smoothstep(0.85, 1.0, abs(fract(rel * 6.0) - 0.5) * 2.0) * rel;
            m += ice * 0.1 * smoothstep(0.8, 1.0, abs(fract(x * 24.0) - 0.5) * 2.0) * rel;
            col = m;
        }
        col += ice * 1.4 * neon(abs(dn), 1.2) * step(1.5 * px, near) * (1.0 + 0.5 * bass);
    } else {
        // the floor: a dark mirror, the grid racing across it
        col = mix(vec3(0.1, 0.01, 0.2), vec3(0.01, 0.0, 0.04), smoothstep(0.0, 0.7, v));

        // the sun mirrored: a trembling column of light under it
        float wob = sin(z * 9.0 - time * 3.0) * 0.012 * v;
        float cw = R * (0.55 + 0.6 * v);
        float column = exp(-pow(abs(x + wob) / cw, 2.0) * 3.0) * exp(-v * 2.2);
        col += mix(vec3(1.0, 0.55, 0.2), vec3(1.0, 0.1, 0.5), smoothstep(0.0, 0.4, v)) * column * 1.3;

        float thick = 0.6 + 2.2 * v;
        float lx = neon(abs(fract(gx + 0.5) - 0.5) / wx, thick);
        float lz = neon(abs(fract(gz + 0.5) - 0.5) / wz, thick);
        // far off the lines crowd: fade to their average glow
        lx = mix(lx, 0.25, smoothstep(0.15, 0.5, wx));
        lz = mix(lz, 0.25, smoothstep(0.15, 0.5, wz));
        float grid = max(lx, lz);
        vec3 gc = mix(violet * 0.8, hot, smoothstep(6.0, 1.5, z));
        gc = mix(gc, vec3(1.0, 0.85, 1.0), flare * smoothstep(3.0, 1.0, z) * 0.8);
        col += gc * grid * (1.1 + 0.8 * flare + 0.4 * level) * smoothstep(0.0, 0.08, v);
    }

    // the horizon: a white-hot line with a haze of rose over both sides
    float dh = abs(y - HY) / px;
    col += vec3(1.0, 0.5, 0.8) * (smoothstep(2.0, 0.0, dh) + 0.35 * exp(-dh * 0.08)) * (1.0 + flare);
    col += hot * 0.2 * exp(-abs(y - HY) * 9.0) * (0.6 + 0.6 * level);

    // hot spots burn to white, the rim darkens
    col += max(col - 1.0, 0.0).gbr * 0.4;
    float vg = 1.0 - 0.35 * dot(uv - 0.5, uv - 0.5) * 2.0;
    fragColor = vec4(clamp(col * vg, 0.0, 1.0), 1.0) * qt_Opacity;
}
