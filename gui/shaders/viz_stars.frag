// stars: a warp starfield, every star worked out where it falls. The
// sky is cut into slices of angle, rings of them, one star in each; a
// star's depth runs down with the flight, so it is thrown out from the
// center, its streak as long as the speed. Behind them a nebula drifts
// toward the viewer and, at warp, a tunnel of light.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;   // seconds, standing still when the style does
    float travel; // how far the field has flown
    float roll;   // how far it has turned
    float speed;  // the flight's speed; past ~1 it is hyperspace
    float beat;   // 1 on a kick, decaying
    float level;  // loudness 0–1
    float aspect; // width / height
    float pixel;  // one pixel, in half-heights
    vec4 tint;    // the cover's color
};

const float TAU = 6.2831853;
const int RINGS = 11;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
vec3 hash3(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.xxy + q.yzz) * q.zyx);
}
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}

void main() {
    vec2 p = (qt_TexCoord0 - 0.5) * vec2(aspect, 1.0) * 2.0;
    float r = length(p);
    float ang = atan(p.y, p.x) + roll;
    // Cruising, streaks are short; a kick stretches them into hyperspace.
    float warp = smoothstep(0.1, 0.8, beat) * smoothstep(0.8, 2.5, speed);
    float tail = speed * mix(0.012, 0.075, smoothstep(0.0, 0.7, beat));

    // The deep: a dark sky tinted by the cover, a nebula rushing past.
    // Colors: the cover's, made vivid, over electric blue and violet.
    float mx = max(max(tint.r, tint.g), tint.b), mn = min(min(tint.r, tint.g), tint.b);
    vec3 vivid = mx - mn > 0.04 ? (tint.rgb - mn) / (mx - mn) : vec3(0.4, 0.6, 1.0);
    vec3 hue = mix(vec3(0.15, 0.5, 1.0), vivid, 0.6);
    vec3 far = mix(vec3(0.45, 0.15, 1.0), vivid, 0.25);
    float lr = log(r + 0.05);
    vec2 np = vec2(cos(ang), sin(ang)) * 1.3 + vec2(lr * 1.6 - travel * 0.35, 0.0);
    float neb = noise(np * 1.7) * 0.65 + noise(np * 3.9 + 7.0) * 0.35;
    neb = smoothstep(0.35, 0.95, neb) * smoothstep(0.0, 0.6, r);
    float neb2 = smoothstep(0.45, 1.0, noise(np.yx * 2.3 - 3.0));
    vec3 col = mix(vec3(0.004, 0.006, 0.025), far * 0.06, smoothstep(1.8, 0.0, r));
    col += (neb * hue * 0.22 + neb2 * far * 0.14) * (0.5 + level) * smoothstep(0.0, 0.8, r);

    // Hyperspace: streaks of light racing along the rays.
    float sl = fract(ang / TAU) * 240.0;
    float ray = hash(vec2(floor(sl), 3.0));
    float thin = smoothstep(0.5, 0.0, abs(fract(sl) - 0.5) - 0.12 * r);
    float run = fract(lr * 0.8 - travel * 0.6 + ray);
    col += warp * pow(ray, 5.0) * thin * smoothstep(0.0, 0.5, run) * smoothstep(1.0, 0.75, run)
         * smoothstep(0.05, 0.6, r) * mix(far, hue, ray) * 1.6;

    // The stars, ring by ring: one in each slice of angle.
    for (int k = 0; k < RINGS; k++) {
        float fk = float(k);
        float n = floor(70.0 + fk * 23.0);
        float a = fract(ang / TAU + fk * 0.1371) * n;
        float s = floor(a);
        float u = hash(vec2(s, fk * 17.0 + 1.0)) - travel / 0.97;
        float z = 0.03 + 0.97 * fract(u);
        vec3 h = hash3(vec2(s + floor(u) * 131.0, fk * 7.0 + 3.0));
        float rho = mix(0.02, 1.9, h.x * h.x);
        float head = rho / z;
        float back = rho / (z + tail);
        float da = (a - s - 0.2 - 0.6 * h.y) / n * TAU;
        float along = clamp(r, back, head);
        vec2 d = vec2(r * da, r - along);
        float dist = length(d);

        float near = 1.0 - z;
        float w = pixel * (0.7 + 3.2 * near * near * near);
        float bright = smoothstep(1.0, 0.7, z) * (0.35 + 1.4 * near * near) * (0.6 + 0.4 * h.z);
        float lead = (head - back) > 1e-4 ? (along - back) / (head - back) : 1.0;
        bright *= mix(0.25, 1.0, lead * lead) / (1.0 + (head - back) / (w * 40.0));
        float core = smoothstep(w, w * 0.3, dist);
        float halo = exp(-dist / (w * 1.6)) * 0.3;
        // The nearest heads glint, a cross of light along and across.
        vec2 q = abs(vec2(r * da, r - head)) / w;
        float glint = smoothstep(0.6, 0.95, near) * (exp(-q.x * 1.2 - q.y * 0.12) + exp(-q.y * 1.2 - q.x * 0.12)) * 0.25;
        // A streak runs from violet at its tail through the cover's color
        // to a white-hot head.
        vec3 c = mix(far, hue, smoothstep(0.0, 0.6, lead) * (0.4 + 0.6 * near));
        c = mix(c, vec3(1.0), smoothstep(0.35, 0.95, near) * smoothstep(0.5, 1.0, lead));
        col += c * bright * core * 1.6 + mix(far, hue, near) * bright * (halo * 1.5 + glint);
    }

    // The vanishing point glows; kicks flash through the whole sky.
    col += hue * (0.08 + 0.4 * beat + 0.25 * warp) * exp(-r * 2.5);
    col += far * 0.12 * (0.4 + level) * exp(-r * 0.8);
    col += vec3(0.85, 0.92, 1.0) * 0.6 * exp(-r * 12.0) * (0.3 + beat + warp);
    col += hue * beat * beat * 0.05;
    col += mix(hue, vec3(1.0), 0.3) * (0.15 + 0.5 * beat) * exp(-abs(p.y) * 40.0 - abs(p.x) * 1.5);

    // Lit like film, softly shaded at the corners.
    col = 1.0 - exp(-col * 1.4);
    col = max(mix(vec3(dot(col, vec3(0.3, 0.55, 0.15))), col, 1.3), 0.0);
    col *= 1.0 - 0.35 * smoothstep(0.9, 2.2, r);
    col += (hash(qt_TexCoord0 * 931.0 + time) - 0.5) / 255.0;
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
