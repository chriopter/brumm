// Fireworks over a harbor city: a night sky with a few stars, the sparks
// and their trails (viz_fireworks_sparks.frag) glowing, the sky and the
// rooftops lit by each burst, and all of it again in the water.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float aspect;
    float beat;    // 1 on a kick, decaying
    vec4 tint;     // the cover's color: the haze over the city
    vec4 s0; vec4 s1; vec4 s2; vec4 s3; vec4 s4; vec4 s5;
    vec4 s6; vec4 s7; vec4 s8; vec4 s9; vec4 s10; vec4 s11;
};
layout(binding = 1) uniform sampler2D sparks;

const float RISE = 0.85;
const float HORIZON = 0.2;  // the waterline, in heights

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

vec3 palette(float i) {
    if (i < 1.0) return vec3(1.0, 0.22, 0.28);
    if (i < 2.0) return vec3(0.25, 1.0, 0.42);
    if (i < 3.0) return vec3(1.0, 0.78, 0.25);
    if (i < 4.0) return vec3(0.3, 0.48, 1.0);
    if (i < 5.0) return vec3(1.0, 0.28, 0.92);
    if (i < 6.0) return vec3(0.25, 0.95, 1.0);
    return max(tint.rgb * 1.3, vec3(0.15));
}

// The light a shell throws on the sky around it as it bursts.
vec3 flash(vec2 p, vec4 s) {
    float a = time - s.z - RISE;
    if (s.z <= 0.0 || a < 0.0 || a > 1.5) return vec3(0.0);
    float seed = floor(s.w);
    vec3 col = mod(seed, 4.0) == 2.0 ? vec3(1.0, 0.62, 0.22) : palette(mod(floor(seed / 4.0), 7.0));
    vec2 d = p - vec2(s.x * aspect, s.y);
    return col * exp(-a * 3.2) * (0.012 + 0.4 * exp(-dot(d, d) * 14.0)) * (0.6 + fract(s.w));
}

// The glowing sparks at uv: sharp, plus two widths of bloom.
vec3 glow(vec2 uv) {
    vec3 c = texture(sparks, uv).rgb;
    c += texture(sparks, uv, 2.5).rgb * 0.7;
    vec2 o = vec2(0.012 / aspect, 0.012);
    c += (texture(sparks, uv + o, 4.5).rgb + texture(sparks, uv - o, 4.5).rgb
        + texture(sparks, uv + vec2(o.x, -o.y), 4.5).rgb + texture(sparks, uv + vec2(-o.x, o.y), 4.5).rgb) * 0.3;
    return c * 2.0;
}

// Rooftops: blocks of a few widths and heights, some with a spire.
float skyline(float x) {
    float col = floor(x * 26.0);
    float h = 0.025 + 0.07 * pow(hash(vec2(col, 3.0)), 2.2);
    float tower = step(0.9, hash(vec2(col, 9.0)));
    float cx = fract(x * 26.0);
    h += tower * 0.05 * (1.0 - smoothstep(0.3, 0.5, abs(cx - 0.5)));
    return HORIZON + h;
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = vec2(uv.x * aspect, 1.0 - uv.y);

    vec3 light = flash(p, s0) + flash(p, s1) + flash(p, s2) + flash(p, s3)
               + flash(p, s4) + flash(p, s5) + flash(p, s6) + flash(p, s7)
               + flash(p, s8) + flash(p, s9) + flash(p, s10) + flash(p, s11);

    // the night: deep blue above, the city's haze below, a few stars
    float up = clamp((p.y - HORIZON) / (1.0 - HORIZON), 0.0, 1.0);
    vec3 sky = mix(vec3(0.05, 0.03, 0.13) + tint.rgb * 0.06, vec3(0.006, 0.008, 0.03), pow(up, 0.6));
    sky += tint.rgb * 0.12 * exp(-up * 7.0) + vec3(0.25, 0.08, 0.2) * 0.12 * exp(-up * 12.0);
    vec2 cell = floor(uv * vec2(aspect, 1.0) * 90.0);
    float st = hash(cell);
    if (st > 0.985) {
        vec2 f = fract(uv * vec2(aspect, 1.0) * 90.0) - 0.5;
        sky += vec3(0.7, 0.8, 1.0) * exp(-dot(f, f) * 40.0) * (0.3 + 0.3 * sin(time * 2.0 + st * 80.0)) * up;
    }

    vec3 c;
    if (p.y >= HORIZON) {
        c = sky + light + glow(uv);
        // the city in front, its roofs catching the bursts, its windows lit
        float roof = skyline(p.x);
        if (p.y < roof) {
            float depth = (roof - p.y) / (roof - HORIZON);
            c = vec3(0.012, 0.012, 0.03) + light * 0.35 * exp(-depth * 5.0) + tint.rgb * 0.02;
            c += (light * 2.0 + vec3(0.05)) * smoothstep(0.004, 0.0, roof - p.y);
            vec2 w = vec2(p.x * 230.0, p.y * 190.0);
            vec2 wf = fract(w);
            float on = step(0.82, hash(floor(w))) * step(0.35, wf.x) * step(0.4, wf.y) * step(0.004, roof - p.y);
            c += vec3(1.0, 0.72, 0.35) * on * (0.35 + 0.15 * hash(floor(w) + 1.0));
        }
    } else {
        // the harbor: the sky and its fireworks mirrored, rippling
        float d = HORIZON - p.y;
        float rip = sin(p.y * 260.0 / (d + 0.05) + time * 2.2) * 0.004 + sin(p.x * 40.0 + time * 1.3) * 0.002;
        vec2 m = vec2(uv.x + rip * (0.4 + d * 4.0), 1.0 - (HORIZON + d * 2.6));
        vec3 refl = glow(m) * 0.9 + light * 0.5;
        float streak = 0.75 + 0.25 * sin(p.y * 700.0 + sin(p.x * 13.0 + time) * 3.0);
        c = vec3(0.004, 0.008, 0.025) + tint.rgb * 0.04 + refl * streak * exp(-d * 3.5);
        // the lit windows' shimmer on the water
        c += vec3(1.0, 0.7, 0.35) * 0.05 * (0.5 + 0.5 * sin(p.x * 260.0 + sin(p.y * 500.0 + time * 2.0) * 2.0)) * exp(-d * 25.0);
        c += light * 0.25 * smoothstep(0.004, 0.0, d);
    }
    c *= 1.0 + beat * 0.08;
    // soft tone: bright, never clipped flat
    c = 1.0 - exp(-c * 1.4);
    fragColor = vec4(c, 1.0) * qt_Opacity;
}
