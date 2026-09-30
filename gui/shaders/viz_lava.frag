// Lava: seven metaballs in a glowing lamp (qml/viz/Lava.qml). Each ball is
// as big as its part of the spectrum, the bass the biggest; where the
// fields add up past one they merge into a blob of glossy wax, lit from
// the lamp's bulb below, with a hot core, a sheen and a halo in the liquid
// around it. A new palette pours in, speckled, over a second and a half.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float phase;  // how far the balls have drifted
    float swell;  // a kick, eased: every ball grows
    float aspect; // width / height
    float pal;    // the palette (0–3)
    float old;    // the palette it pours in over
    float mixing; // 0 → 1 as it pours
    float beat;   // 1 on a kick, decaying
    vec4 tint;    // the cover's color
};
layout(binding = 1) uniform sampler2D spectrum;

// per ball: x and y frequency, x and y phase (as the terminal's)
const vec4 path[7] = vec4[7](
    vec4(0.31, 0.43, 0.0, 1.7), vec4(0.23, 0.37, 2.1, 0.4),
    vec4(0.41, 0.29, 4.0, 2.9), vec4(0.19, 0.53, 1.1, 5.2),
    vec4(0.37, 0.21, 5.5, 3.3), vec4(0.27, 0.47, 3.2, 0.9),
    vec4(0.45, 0.33, 0.7, 4.4));

// per palette: the liquid, the wax's skin, its core, and the hot white
const vec3 pals[16] = vec3[16](
    vec3(0.12, 0.00, 0.14), vec3(0.70, 0.00, 0.35), vec3(1.00, 0.30, 0.12), vec3(1.0, 0.85, 0.50),
    vec3(0.14, 0.02, 0.00), vec3(0.60, 0.03, 0.00), vec3(1.00, 0.42, 0.00), vec3(1.0, 0.90, 0.40),
    vec3(0.00, 0.03, 0.14), vec3(0.02, 0.12, 0.70), vec3(0.10, 0.70, 1.00), vec3(0.80, 1.0, 1.00),
    vec3(0.00, 0.08, 0.05), vec3(0.02, 0.40, 0.08), vec3(0.55, 1.00, 0.10), vec3(1.0, 1.0, 0.70));

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = vec2(uv.x * aspect, uv.y);
    float base = 0.12 * min(aspect, 1.0);

    // the field, and its gradient for the light
    float f = 0.0;
    vec2 g = vec2(0.0);
    for (int k = 0; k < 7; k++) {
        vec4 b = path[k];
        vec2 c = vec2(aspect * (0.5 + 0.32 * sin(phase * b.x * 2.0 + b.z)),
                      0.5 + 0.34 * sin(phase * b.y * 2.0 + b.w));
        float band = texture(spectrum, vec2((float(k) / 6.0 * 63.0 + 0.5) / 64.0, 1.5 / 4.0)).r;
        float r = base * (0.6 + 1.1 * band + swell);
        vec2 d = p - c;
        float q = dot(d, d) + 1e-4;
        float e = r * r / q;
        f += e;
        g -= 2.0 * e / q * d;
    }

    // the palette, a new one poured in from below in curling tongues
    float n = noise(uv * vec2(aspect, 1.0) * 3.0) * 0.65 + noise(uv * vec2(aspect, 1.0) * 8.0) * 0.35;
    float w = smoothstep(-0.06, 0.06, mixing * 1.4 - 0.1 - 0.6 * (n + 1.0 - uv.y));
    int i = int(pal + 0.5) * 4, o = int(old + 0.5) * 4;
    vec3 liquid = mix(mix(pals[o], pals[i], w), tint.rgb * 0.15, 0.3);
    vec3 skin = mix(pals[o + 1], pals[i + 1], w);
    vec3 core = mix(pals[o + 2], pals[i + 2], w);
    vec3 white = mix(pals[o + 3], pals[i + 3], w);

    // the lamp: its liquid lit by the bulb below, brighter on a kick
    float bulb = exp(-(1.0 - uv.y) * 2.2 - pow(abs(uv.x - 0.5) * 2.0, 2.0) * 1.5) * (0.9 + 0.5 * beat);
    vec3 col = liquid * (0.35 + 1.3 * bulb) + skin * 0.12 * bulb * bulb;
    // the halo each blob throws into the liquid
    float halo = smoothstep(0.3, 1.0, f);
    col += skin * halo * halo * 0.4 * (0.7 + 0.6 * beat);

    // the wax: depth 0 at its skin, toward 1 deep inside; a dome over it
    float depth = 1.0 - 1.0 / max(f, 1e-3);
    float inside = smoothstep(0.0, 0.015, depth);
    if (inside > 0.0) {
        vec2 slope = g / (f * f) / (2.0 * sqrt(max(depth, 1e-3))) * 0.03;
        vec3 n = normalize(vec3(-slope, 1.0));
        vec3 l = normalize(vec3(-0.45, -0.6, 0.65)); // the room's light, upper left
        float diff = max(dot(n, l), 0.0);
        float up = max(dot(n, normalize(vec3(0.0, 1.0, 0.3))), 0.0); // the bulb's, from beneath
        float rim = pow(1.0 - n.z, 1.5);
        // lit through: glowing where it is thick, deep and dark at its skin
        float thick = pow(n.z, 5.0) * smoothstep(0.0, 0.4, depth);
        vec3 wax = mix(skin * 0.4, core * 1.2, thick);
        wax += white * thick * smoothstep(0.5, 1.0, depth) * (0.2 + 0.5 * beat);
        vec3 lit = wax * (0.7 + 0.5 * diff)
                 + core * up * up * 1.4 * (0.4 + bulb)
                 + skin * rim * 1.2;
        // the gloss: a sharp window of light and a soft sheen
        vec3 h = normalize(l + vec3(0.0, 0.0, 1.0));
        float nh = max(dot(n, h), 0.0);
        lit += vec3(1.0) * smoothstep(0.965, 0.99, nh) * 1.2 + white * pow(nh, 12.0) * 0.15;
        col = mix(col, lit, inside);
    }

    // the glass: a chrome streak down each side, and its shadowed edges
    float x = uv.x;
    col += vec3(1.0) * 0.10 * exp(-abs(x - 0.12) * 60.0) + vec3(1.0) * 0.05 * exp(-abs(x - 0.9) * 90.0);
    col += tint.rgb * 0.06 * exp(-abs(x - 0.16) * 25.0);
    col *= 1.0 - 0.55 * pow(abs(x - 0.5) * 2.0, 3.0);
    col *= 1.0 - 0.35 * smoothstep(0.35, 0.0, uv.y);

    // bright spots bloom toward white rather than clip
    col = 1.0 - exp(-col * 1.25);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
