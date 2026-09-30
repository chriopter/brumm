// Ridge: Unknown Pleasures in neon. The spectrum's history (viz_ridge_push)
// recedes toward a horizon in perspective, each ridge a sheet of dark glass
// with a line of light along its crest, hiding what lies behind it: the
// front line white-hot and swelling on a kick, the ones behind cooling
// through cyan and blue into the cover's color as they go. One pass.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float slide;  // how far the history has slid toward the next row, 0–1
    float beat;
    float bass;
    float level;
    float aspect; // width / height
    vec4 tint;    // the cover's color
};
layout(binding = 1) uniform sampler2D hist;

const int LINES = 41;      // the live line and the forty behind it
const float HZ = 0.14;     // where the lines meet, from the top
const float BASE = 0.86;    // the front line's foot
const float AMP = 0.52;    // the front line's reach, in heights
const float KP = 3.0 / 40.0;

// color of a line at depth rank 0 (front) … 1 (the horizon)
vec3 shade(float r) {
    vec3 c = mix(vec3(0.75, 0.95, 1.0), vec3(0.1, 0.8, 1.0), smoothstep(0.0, 0.15, r));
    c = mix(c, vec3(0.2, 0.35, 1.0), smoothstep(0.15, 0.45, r));
    c = mix(c, mix(vec3(0.55, 0.2, 1.0), tint.rgb, 0.4), smoothstep(0.45, 0.85, r));
    return c * mix(1.0, 0.18, pow(r, 0.8));
}

void main() {
    vec2 uv = qt_TexCoord0;
    float px = fwidth(uv.y);
    float y = uv.y;

    vec3 light = vec3(0.0); // the crests in front, glowing over what's behind
    vec3 ground = vec3(0.0);
    float glass = 0.0; // how much of the pixel a ridge's glass hides
    for (int i = 0; i < LINES; i++) {
        float d = i == 0 ? 0.0 : float(i - 1) + slide;
        float s = 1.0 / (1.0 + d * KP);
        float base = HZ + (BASE - HZ) * s;
        float A = AMP * s * (i == 0 ? 1.0 + 0.25 * beat : 1.0);
        float reach = (30.0 * s + 6.0) * px;
        if (y < base - A - reach)
            continue; // too far above for this line to touch
        float hw = 0.5 * (0.3 + 0.68 * s);
        float q = (uv.x - 0.5) / (2.0 * hw) + 0.5;
        if (q < 0.0 || q > 1.0)
            continue;
        float v = texture(hist, vec2(q * (127.0 / 128.0) + 0.5 / 128.0, (float(i) + 0.5) / 41.0)).r;
        float dist = (y - (base - A * v)) / px; // pixels below the crest
        float r = d / 40.0;
        vec3 c = shade(r);
        float ends = smoothstep(0.0, 0.05, min(q, 1.0 - q));
        float w = 0.5 + 1.3 * s;
        float ad = abs(dist);
        float hot = i == 0 ? 1.0 + 1.2 * beat : 1.0;
        float core = smoothstep(w + 1.0, w - 0.5, ad);
        light += (mix(c, vec3(1.0), 0.55 * (1.0 - r)) * core * 1.4
                  + c * (0.55 * exp(-ad / (1.5 + 3.0 * s)) + 0.2 * exp(-ad / (4.0 + 22.0 * s)))) * ends * hot;
        if (dist > 0.0) {
            // the ridge's glass: dark, lit from its crest down, a faint
            // sheen where it rises highest
            ground = c * (0.22 * exp(-dist / (6.0 + 40.0 * s)) + 0.05 * v) + vec3(0.0, 0.0, 0.015);
            glass = ends;
            break;
        }
    }

    // the sky behind: deep blue into black, a haze on the horizon and a
    // light behind the middle that breathes with the bass
    vec3 sky = mix(vec3(0.0, 0.01, 0.05), mix(vec3(0.03, 0.02, 0.12), tint.rgb * 0.2, 0.3), smoothstep(0.0, 0.5, y));
    sky += mix(vec3(0.3, 0.4, 1.0), tint.rgb, 0.4) * 0.35 * exp(-abs(y - HZ) * 14.0);
    vec2 p = vec2((uv.x - 0.5) * aspect, y - 0.4);
    sky += mix(vec3(0.2, 0.5, 1.0), tint.rgb, 0.3) * (0.12 + 0.3 * bass) * exp(-dot(p, p) * 6.0);
    ground = mix(sky, ground, glass);

    vec3 col = ground + light;
    col += max(col - 1.0, 0.0).gbr * 0.3;
    float vg = 1.0 - 0.4 * dot(uv - 0.5, uv - 0.5) * 2.0;
    fragColor = vec4(clamp(col * vg, 0.0, 1.0), 1.0) * qt_Opacity;
}
