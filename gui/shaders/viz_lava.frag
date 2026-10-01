// Lava, the lamp (qml/viz/Lava.qml): everything but the wax's shape, which
// comes in a texture (viz_lava_wax.frag: normal, thickness, cover). Behind
// it the liquid, lit by the bulb under the floor: rays fanning up, caustics
// on the back wall, bubbles rising on the highs, all of it wavering in the
// heat. The wax is lit through from below — glowing where it is thick, a
// burning rim where it is thin — glossed with a window of light and the
// bulb's reflection, and lets a little of the liquid show bent through it.
// Around it the glass with its streaks, and a chrome foot and cap that
// mirror the glow. The wax blooms from the texture's mip levels. A new
// palette pours in from below, in curling tongues, over a second and a half.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float phase;  // how far the balls have drifted
    float tick;   // seconds it has run
    float aspect; // width / height
    float pal;    // the palette (0–5)
    float old;    // the palette it pours in over
    float mixing; // 0 → 1 as it pours
    float beat;   // 1 on a kick, decaying
    float bass;
    float high;
    float level;
    vec4 tint;    // the cover's color
};
layout(binding = 1) uniform sampler2D shape;

// per palette: the liquid, the wax's skin, its core, and the hot white
const vec3 pals[24] = vec3[24](
    vec3(0.16, 0.00, 0.30), vec3(0.95, 0.00, 0.42), vec3(1.00, 0.38, 0.05), vec3(1.0, 0.88, 0.55),
    vec3(0.00, 0.05, 0.34), vec3(0.00, 0.30, 1.00), vec3(0.10, 0.85, 1.00), vec3(0.85, 1.0, 1.00),
    vec3(0.26, 0.02, 0.00), vec3(0.90, 0.04, 0.00), vec3(1.00, 0.55, 0.00), vec3(1.0, 0.95, 0.50),
    vec3(0.00, 0.16, 0.20), vec3(0.05, 0.75, 0.10), vec3(0.70, 1.00, 0.05), vec3(1.0, 1.0, 0.70),
    vec3(0.10, 0.02, 0.36), vec3(1.00, 0.45, 0.00), vec3(1.00, 0.80, 0.10), vec3(1.0, 1.0, 0.80),
    vec3(0.00, 0.20, 0.26), vec3(1.00, 0.00, 0.55), vec3(1.00, 0.35, 0.60), vec3(1.0, 0.85, 0.95));

const float TAU = 6.2831853;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}

// light gathered by a moving surface into bright wandering lines
float caustics(vec2 p) {
    vec2 c = mod(p * TAU, TAU) - 250.0, i = c;
    float sum = 1.0;
    for (int n = 0; n < 4; n++) {
        float t = tick * 0.45 * (1.0 - 3.5 / float(n + 1));
        i = c + vec2(cos(t - i.x) + sin(t + i.y), sin(t - i.y) + cos(t + i.x));
        sum += 1.0 / length(vec2(c.x / (sin(i.x + t) / 0.005), c.y / (cos(i.y + t) / 0.005)));
    }
    return pow(abs(1.17 - pow(sum / 4.0, 1.4)), 8.0);
}

// one sheet of rising bubbles, each a little glass ball: x its light, y its cover
vec2 bubbles(vec2 p, float cells, float rise, float seed, float many) {
    vec2 q = vec2(p.x * cells + seed * 7.3, p.y * cells + tick * rise);
    vec2 cell = floor(q), f = fract(q) - 0.5;
    float h = hash(cell + seed);
    if (h > many) return vec2(0.0);
    float r = 0.05 + 0.11 * hash(cell + seed + 3.1);
    vec2 o = f - vec2((hash(cell + seed + 5.7) - 0.5) * 0.4 + 0.08 * sin(tick * 3.0 + h * 40.0), (h / many - 0.5) * 0.3);
    float d = length(o) / r;
    if (d > 1.0) return vec2(0.0);
    float glint = smoothstep(0.35, 0.0, length(o / r + vec2(0.35, 0.4)));
    return vec2(pow(d, 4.0) * 0.55 + glint * 1.3, smoothstep(1.0, 0.85, d));
}

void main() {
    vec2 uv = qt_TexCoord0;
    float e = abs(uv.x - 0.5) * 2.0; // 0 at the glass's middle, 1 at its edge

    // the wax in front of this pixel
    vec4 w = texture(shape, uv);
    float cover = smoothstep(0.25, 0.75, w.a);
    vec2 nxy = (w.rg * 2.0 - 1.0) * step(0.02, w.a);
    vec3 n = vec3(nxy, sqrt(max(1.0 - dot(nxy, nxy), 0.0)));
    float thick = w.b;

    // the palette, a new one poured in from below
    vec2 p = vec2(uv.x * aspect, uv.y);
    float tongues = noise(p * 3.0 + phase) * 0.65 + noise(p * 8.0 - phase) * 0.35;
    float pour = smoothstep(-0.06, 0.06, mixing * 1.4 - 0.1 - 0.6 * (tongues + 1.0 - uv.y));
    int i = int(pal + 0.5) * 4, o = int(old + 0.5) * 4;
    vec3 liquid = mix(mix(pals[o], pals[i], pour), tint.rgb * 0.25, 0.2);
    vec3 skin = mix(pals[o + 1], pals[i + 1], pour);
    vec3 core = mix(pals[o + 2], pals[i + 2], pour);
    vec3 white = mix(pals[o + 3], pals[i + 3], pour);

    // the liquid is seen bent through the wax, and wavers in the heat
    vec2 at = uv + vec2(-n.x, n.y) * 0.07 * cover;
    float heat = smoothstep(0.1, 1.0, at.y);
    at += (0.002 + 0.004 * heat) * vec2(sin(at.y * 37.0 - tick * 4.0) + sin(at.y * 71.0 - tick * 6.3),
                                         cos(at.x * 43.0 + tick * 3.1));
    vec2 q = vec2((at.x - 0.5) * aspect, at.y);

    // the bulb under the floor, its light fanning up in rays
    vec2 toBulb = vec2(q.x, 1.06 - q.y);
    float dist = length(toBulb);
    float ang = atan(toBulb.x, toBulb.y);
    float flare = 0.75 + 1.1 * beat + 0.5 * bass;
    float bulb = exp(-dist * 1.9) * flare;
    float rays = 0.5 + 0.5 * sin(ang * 11.0 + tick * 0.35) * sin(ang * 5.0 - tick * 0.23 + sin(ang * 3.0 + tick * 0.5));
    vec3 col = liquid * (0.22 + 1.9 * bulb + 0.35 * (1.0 - at.y));
    col += mix(skin, core, 0.6) * bulb * bulb * (0.5 + 1.6 * rays * rays);
    col += white * pow(bulb, 4.0) * 0.9;
    // caustics on the back wall, livelier the louder
    float ca = caustics(q * 0.55 + vec2(0.0, tick * 0.02));
    col += mix(core, white, 0.4) * ca * (0.25 + 0.9 * level + 0.5 * beat) * (0.5 + 1.4 * bulb);
    // bubbles, more of them on the highs
    float many = 0.10 + 0.75 * high;
    vec2 b = bubbles(q, 9.0, 1.3, 1.0, many)
           + bubbles(q, 15.0, 1.9, 2.0, many) * 0.8
           + bubbles(q, 24.0, 2.6, 3.0, many) * 0.6;
    col += (white * 0.55 + liquid) * b.x * (0.5 + bulb);

    // the wax blooms into the liquid around it
    float halo = textureLod(shape, uv, 3.0).a * 0.35 + textureLod(shape, uv, 4.5).a * 0.45 + textureLod(shape, uv, 6.0).a * 0.5;
    col += mix(skin, core, halo) * halo * halo * (0.75 + 0.9 * beat);

    if (cover > 0.0) {
        float warm = smoothstep(0.0, 1.0, uv.y); // nearer the bulb, hotter
        float fres = pow(1.0 - n.z, 2.5);
        // lit through: glowing where it is thick, deep at its skin
        float through = clamp(pow(thick, 1.6) * (0.45 + 0.7 * warm) + 0.2 * warm * warm, 0.0, 1.0);
        vec3 wax = mix(skin * skin * 0.5 + skin * 0.12, core * 1.25, through);
        wax += white * pow(through, 3.0) * (0.35 + 0.9 * beat);
        // the bulb's light from beneath, and a rim that burns
        float under = max(-n.y, 0.0);
        wax += core * under * under * (0.5 + 1.4 * bulb + 0.8 * beat);
        wax += (skin * 1.6 + white * 0.5 * fres) * fres * (0.8 + 0.6 * beat);
        // a little of the liquid shows through the thin parts
        wax = mix(wax, col * 1.4, 0.22 * (1.0 - thick));
        // the gloss: the room mirrored in it — a window up left, a strip of
        // light along the horizon, the bulb below
        vec3 r = vec3(2.0 * n.z * n.xy, 2.0 * n.z * n.z - 1.0);
        float win = smoothstep(0.10, 0.16, -r.x) * smoothstep(0.74, 0.68, -r.x)
                  * smoothstep(0.16, 0.22, r.y) * smoothstep(0.80, 0.74, r.y)
                  * (1.0 - 0.8 * smoothstep(0.035, 0.02, abs(r.y - 0.48)) - 0.8 * smoothstep(0.03, 0.015, abs(-r.x - 0.42)))
                  * (0.55 + 0.45 * smoothstep(0.16, 0.8, r.y));
        float strip = smoothstep(0.10, 0.02, abs(r.y - 0.05 - 0.2 * r.x)) * smoothstep(-0.2, 0.9, r.x);
        float glint = pow(max(dot(n, normalize(vec3(-0.35, 0.5, 0.8))), 0.0), 220.0);
        float below = smoothstep(-0.55, -0.95, r.y);
        wax += vec3(1.0) * (win * 0.7 + glint * 1.5) + white * (strip * 0.22 + below * (0.25 + 0.6 * beat) * warm);
        col = mix(col, wax, cover);
    }

    // the bulb's flash on a kick: a streak across the floor
    col += white * beat * 0.5 * exp(-abs(uv.y - 0.93) * 55.0) * exp(-e * e * 2.5);

    // the glass: streaks of the room down its sides, darkening where it bends away
    float glass = 0.30 * exp(-abs(uv.x - 0.045) * 140.0) + 0.16 * exp(-abs(uv.x - 0.965) * 200.0)
                + 0.07 * smoothstep(0.05, 0.0, abs(uv.x - 0.135)) * (1.0 - uv.y)
                + 0.04 * smoothstep(0.012, 0.0, abs(uv.x - 0.19)) * (1.0 - uv.y);
    col += (vec3(1.0) * 0.8 + tint.rgb * 0.4) * glass;
    col += skin * 0.25 * pow(e, 6.0);
    col *= 1.0 - 0.62 * pow(e, 3.5);

    // chrome, foot and cap: curved, banded as chrome is, and in it the
    // lamp's glow mirrored and squeezed
    float bow = 0.030 * (1.0 - e * e);
    float foot = uv.y - (0.945 - bow), cap = (0.042 + bow) - uv.y;
    float metal = max(foot, cap);
    if (metal > 0.0) {
        float into = min(metal * 14.0, 1.0);
        float side = (uv.x - 0.5) * 2.0;
        float bands = 0.10 + 0.9 * pow(0.5 + 0.5 * sin(asin(clamp(side, -1.0, 1.0)) * 4.0 + 0.9), 3.0);
        float my = foot > 0.0 ? 0.945 - bow - foot * 6.0 : 0.042 + bow + cap * 6.0;
        vec4 m = textureLod(shape, vec2(uv.x + side * metal * 0.4, my), 2.5);
        vec3 chrome = vec3(0.55, 0.60, 0.70) * bands * (0.35 + 0.65 * into)
                    + liquid * 0.9 * (1.0 - into)
                    + mix(skin, core, m.b) * m.a * 0.7 * into
                    + white * exp(-metal * 90.0) * (foot > 0.0 ? 0.7 + 1.5 * beat : 0.25)
                    + core * exp(-metal * 22.0) * (foot > 0.0 ? 0.5 + 0.8 * beat : 0.15);
        chrome *= 1.0 - 0.7 * pow(e, 3.0);
        col = mix(col, chrome, smoothstep(0.0, 0.003, metal));
    }

    // bright spots bloom toward white rather than clip
    col = 1.0 - exp(-col * 1.3);
    col += (hash(uv * 733.0 + tick) - 0.5) / 255.0;
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
