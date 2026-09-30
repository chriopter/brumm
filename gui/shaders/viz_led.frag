// Led: a hi-fi graphic equalizer behind smoked glass. Columns of LED
// segments, green low, amber, then red, the leading one brightest and
// every lit one glowing into the dark around it; unlit segments show
// faintly, a white peak-hold LED floats on each column, a kick lights the
// columns whole. Below, the panel mirrored on a glossy floor.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float aspect;  // width / height
    float cols;    // columns
    float rows;    // segments in a column
    float beat;    // 1 on a kick, decaying
    vec4 tint;     // the cover's color, in the glass
};
layout(binding = 1) uniform sampler2D spectrum;

// The panel, in heights: left, bottom, width, height.
const float BOTTOM = 0.3, HEIGHT = 0.56;

// A band's level and falling peak for column i, averaged over its bands.
vec2 level(float i) {
    float x = (i + 0.5) / cols;
    float w = 0.3 / cols;
    // the bands and their smoothed copy, half and half: quick, not jumpy
    float v = texture(spectrum, vec2(x - w, 0.125)).r + texture(spectrum, vec2(x + w, 0.125)).r
            + texture(spectrum, vec2(x - w, 0.375)).r + texture(spectrum, vec2(x + w, 0.375)).r;
    v = v * 0.25 * 1.25;
    float pk = max(texture(spectrum, vec2(x, 0.625)).r * 1.1, v);
    return clamp(vec2(v, pk), 0.0, 1.0);
}

// The LED color at height f (0 bottom, 1 top).
vec3 zone(float f) {
    vec3 green = vec3(0.1, 1.0, 0.35), lime = vec3(0.6, 1.0, 0.15);
    vec3 amber = vec3(1.0, 0.68, 0.08), red = vec3(1.0, 0.12, 0.1);
    if (f < 0.45) return mix(green, lime, f / 0.45);
    if (f < 0.6) return mix(lime, amber, (f - 0.45) / 0.15);
    if (f < 0.85) return amber;
    return mix(amber, red, smoothstep(0.85, 0.9, f));
}

// The display at p (panel units: x 0–cols, y 0–rows), lit and glowing.
vec3 display(vec2 p) {
    vec3 c = vec3(0.0);
    float ci = floor(p.x), ri = floor(p.y);
    bool whole = beat > 0.45;
    // this segment and its neighbors: each lit one glows onto this pixel
    for (int dx = -1; dx <= 1; dx++) {
        float i = ci + float(dx);
        if (i < 0.0 || i >= cols) continue;
        vec2 lv = level(i);
        float lit = floor(lv.x * rows + 0.5);
        float pr = lv.y > 0.02 ? min(floor(lv.y * rows + 0.5), rows) - 1.0 : -1.0;
        for (int dy = -1; dy <= 1; dy++) {
            float r = ri + float(dy);
            if (r < 0.0 || r >= rows) continue;
            // the segment: a rounded bar filling most of its cell
            vec2 q = p - vec2(i + 0.5, r + 0.5);
            vec2 hs = vec2(0.36, 0.3);
            vec2 e = abs(q) - hs + 0.08;
            float sd = length(max(e, 0.0)) + min(max(e.x, e.y), 0.0) - 0.08;
            float body = smoothstep(0.03, -0.03, sd);
            float halo = exp(-max(sd, 0.0) * 6.0) * step(0.0, sd);
            float f = (r + 0.5) / rows;
            vec3 col = zone(f);
            float on = 0.0;
            if (r == pr && r >= lit - 1.0) { col = vec3(0.9, 0.95, 1.0); on = 1.6; }
            else if (r < lit) on = (whole || r == lit - 1.0) ? 1.5 : 0.8;
            if (on > 0.0) {
                // a hot core and a lens-like gloss across the top of each LED
                float core = exp(-dot(q / hs, q / hs) * 1.5);
                float gloss = smoothstep(0.08, 0.22, q.y) * body * 0.12;
                c += col * body * on * (0.45 + 1.1 * core) + vec3(gloss) * on;
                c += col * halo * on * 0.35;
            } else if (dx == 0 && dy == 0) {
                c += col * body * 0.055;
            }
        }
        // the whole column's light, spilling wide
        float top = lv.x * rows;
        float dxw = abs(p.x - (i + 0.5)) - 0.36;
        float dyw = max(p.y - top, 0.0);
        c += zone(clamp(p.y / rows, 0.0, 1.0)) * exp(-max(dxw, 0.0) * 1.6 - dyw * 0.9) * 0.12 * lv.x * (whole ? 2.0 : 1.0);
    }
    return c;
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = vec2(uv.x * aspect, 1.0 - uv.y);
    float W = min(aspect * 0.84, HEIGHT * cols / rows * 1.9);
    float L = (aspect - W) * 0.5;
    vec2 cell = vec2(W / cols, HEIGHT / rows);

    // the room: black, a haze of the cover's color behind the panel
    vec2 cp = p - vec2(aspect * 0.5, BOTTOM + HEIGHT * 0.5);
    vec3 c = vec3(0.01, 0.01, 0.018) + tint.rgb * 0.12 * exp(-dot(cp, cp) * 2.5);

    float pad = 0.035;
    vec2 lo = vec2(L - pad, BOTTOM - pad), hi = vec2(L + W + pad, BOTTOM + HEIGHT + pad);
    if (p.y >= BOTTOM - pad * 1.5) {
        // the panel: smoked glass in a chrome rim
        vec2 d = max(lo - p, p - hi);
        float sd = max(d.x, d.y);
        if (sd < 0.0) {
            vec2 g = (p - lo) / (hi - lo);
            c = mix(vec3(0.02, 0.022, 0.03), vec3(0.005, 0.006, 0.01), g.y) + tint.rgb * 0.03;
            vec2 dp = (p - vec2(L, BOTTOM)) / cell;
            c += display(dp) * (1.0 + beat * 0.25);
            // the glass: a curved sheen over the top, fine scan lines
            float edge = g.y + 0.07 * sin(g.x * 3.1416);
            float sheen = smoothstep(0.6, 0.63, edge) * mix(0.015, 0.11, smoothstep(0.63, 1.0, edge));
            c += vec3(0.8, 0.9, 1.0) * sheen;
            c *= 0.94 + 0.06 * sin(uv.y * 900.0);
        }
        // the rim, chrome: light on top, dark below, a thin bright edge
        float rim = smoothstep(0.0, -0.002, sd) * smoothstep(-0.02, -0.018, sd);
        float ny = clamp((p.y - lo.y) / (hi.y - lo.y), 0.0, 1.0);
        float band = (sd + 0.02) / 0.02;  // 0 inside … 1 outside edge of the rim
        // polished metal: bands of sky and ground, a hard highlight, a dark inner lip
        float m = pow(0.5 + 0.5 * sin(ny * 6.0 + band * 3.5 - 1.2), 3.0);
        vec3 chrome = mix(vec3(0.05, 0.055, 0.07), vec3(0.95, 0.97, 1.0), m * (0.35 + 0.65 * ny));
        chrome += vec3(1.0) * exp(-pow((band - 0.7) * 9.0, 2.0)) * (0.1 + 0.9 * ny);
        chrome *= smoothstep(0.0, 0.2, band);
        c = mix(c, chrome + tint.rgb * 0.08, rim);
        // the rim's bright outer line and the light it spills
        c += tint.rgb * 0.06 * exp(-max(sd, 0.0) * 60.0) * step(0.0, sd);
    } else {
        // the floor: glossy black, the panel mirrored and fading
        float d = (BOTTOM - pad * 1.5) - p.y;
        vec2 m = vec2(p.x, BOTTOM - pad * 1.5 + d);
        vec3 refl = vec3(0.0);
        if (m.x > L && m.x < L + W) refl = display((m - vec2(L, BOTTOM)) / cell);
        c += refl * 0.35 * exp(-d * 9.0);
        c += tint.rgb * 0.04 * exp(-d * 6.0);
    }
    // soft tone, and a vignette
    // soft tone that keeps the colors deep; the hottest go white
    float peak = max(c.r, max(c.g, c.b));
    c *= (1.0 - exp(-peak * 1.5)) / max(peak, 1e-4);
    c = mix(c, vec3(1.0), smoothstep(1.4, 4.0, peak) * 0.55);
    c *= 1.0 - 0.35 * dot(uv - 0.5, uv - 0.5);
    fragColor = vec4(c, 1.0) * qt_Opacity;
}
