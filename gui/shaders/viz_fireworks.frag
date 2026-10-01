// Fireworks over a harbor city: a night sky with twinkling stars and
// clouds, the sparks and their trails (viz_fireworks_sparks.frag) blooming
// and streaking in the lens, the smoke they leave (viz_fireworks_smoke.frag)
// lit from inside by each new burst, the whole sky flashing with it, a
// skyline whose windows are the spectrum, and all of it again in rippling
// water, behind a few boats.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float aspect;
    float beat;    // 1 on a kick, decaying
    float glitter; // the highs: the sparks glitter
    float level;   // how loud
    vec4 tint;     // the cover's color: the haze over the city
    mat4 m0; mat4 m1; mat4 m2; mat4 m3; mat4 m4;  // the shells, as the sparks have them
};
layout(binding = 1) uniform sampler2D sparks;
layout(binding = 2) uniform sampler2D smoke;
layout(binding = 3) uniform sampler2D spectrum;

#define ROW(m, r) vec4(m[0][r], m[1][r], m[2][r], m[3][r])

const float HORIZON = 0.22;  // the waterline, in heights

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
               mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

vec3 palette(float i) {
    if (i < 1.0) return vec3(1.0, 0.12, 0.2);
    if (i < 2.0) return vec3(0.15, 1.0, 0.35);
    if (i < 3.0) return vec3(1.0, 0.7, 0.15);
    if (i < 4.0) return vec3(0.2, 0.4, 1.0);
    if (i < 5.0) return vec3(1.0, 0.18, 0.9);
    if (i < 6.0) return vec3(0.15, 0.9, 1.0);
    return max(tint.rgb * 1.3, vec3(0.15));
}

// The light a bursting shell throws: on the air around it and, thinner, on
// the whole sky; and the streak it draws across the lens.
void flash(vec2 p, vec4 s, inout vec3 light, inout vec3 streak) {
    float a = time - s.z;
    if (s.z <= 0.0 || a < 0.0 || a > 1.8) return;
    float seed = floor(s.w);
    vec3 col = mod(seed, 8.0) == 2.0 ? vec3(1.0, 0.6, 0.2) : palette(mod(floor(seed / 8.0), 7.0));
    vec2 d = p - vec2(s.x * aspect, fract(s.y));
    float e = exp(-a * 3.0) * (0.4 + 1.2 * fract(s.w));
    light += col * e * (0.03 + 0.5 * exp(-dot(d, d) * 11.0));
    streak += mix(col, vec3(0.5, 0.7, 1.0), 0.5) * e * exp(-a * 4.0)
            * exp(-d.y * d.y * 12000.0) * exp(-abs(d.x) * 3.5);
}

// The sparks at uv through the lens: sharp and glittering, two widths of
// bloom, and a soft streak sideways.
vec3 glow(vec2 uv, float soft) {
    vec2 cell = floor(uv * vec2(aspect, 1.0) * 420.0);
    float g = hash(cell + floor(time * 24.0) * 7.3);
    vec3 c = texture(sparks, uv, soft).rgb * (1.0 + (0.6 + 4.0 * glitter) * step(0.88, g));
    c += texture(sparks, uv, 2.5 + soft).rgb * 0.7;
    vec2 o = vec2(0.012 / aspect, 0.012);
    c += (texture(sparks, uv + o, 4.5).rgb + texture(sparks, uv - o, 4.5).rgb
        + texture(sparks, uv + vec2(o.x, -o.y), 4.5).rgb + texture(sparks, uv + vec2(-o.x, o.y), 4.5).rgb) * 0.3;
    vec3 st = texture(sparks, uv + vec2(0.012, 0.0), 3.0).rgb + texture(sparks, uv - vec2(0.012, 0.0), 3.0).rgb
            + (texture(sparks, uv + vec2(0.035, 0.0), 4.0).rgb + texture(sparks, uv - vec2(0.035, 0.0), 4.0).rgb) * 0.6
            + (texture(sparks, uv + vec2(0.07, 0.0), 5.0).rgb + texture(sparks, uv - vec2(0.07, 0.0), 5.0).rgb) * 0.3;
    c += st * st * vec3(0.5, 0.7, 1.3) * 1.6 + st * 0.08;
    return c * 2.0;
}

// Rooftops: blocks of a few widths and heights, some with a spire.
float skyline(float x, float n, float tall, float row) {
    float col = floor(x * n);
    float h = 0.02 + tall * pow(hash(vec2(col, row)), 2.2);
    float cx = fract(x * n);
    h += step(0.88, hash(vec2(col, row + 6.0))) * 0.045 * (1.0 - smoothstep(0.0, 0.12, abs(cx - 0.5)));
    return HORIZON + h;
}

// A sailboat: hull, mast and sail, as one dark shape. p from its waterline.
float boat(vec2 p, float s) {
    p /= s;
    float hull = step(abs(p.x), 0.03 - 0.5 * (0.008 - p.y)) * step(0.0, p.y) * step(p.y, 0.008);
    float mast = step(abs(p.x), 0.0007) * step(0.0, p.y) * step(p.y, 0.062);
    float sail = step(0.002, p.x) * step(0.012, p.y) * step(p.x * 2.1 + p.y, 0.058);
    return max(hull, max(mast, sail));
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = vec2(uv.x * aspect, 1.0 - uv.y);

    // below the waterline, look at the sky mirrored and rippling instead
    float d = HORIZON - p.y;
    bool water = d > 0.0;
    vec2 sp = p;
    float soft = 0.0, glint = 1.0;
    if (water) {
        float far = 1.0 / (d + 0.025);
        float rip = noise(vec2(p.x * 9.0 * far * 0.12, far * 1.3 + time * 0.9)) - 0.5;
        float rip2 = noise(vec2(p.x * 3.0 * far, far * 5.0 - time * 1.6)) - 0.5;
        sp.x += (rip * 0.05 + rip2 * 0.012) * (0.25 + d * 5.0);
        sp.y = HORIZON + d * 2.5 + (rip * 0.5 + rip2 * 0.25) * d;
        glint = 0.55 + 0.9 * (rip2 + 0.5) * (rip + 0.5);
        soft = 0.8 + d * 6.0;
    }
    vec2 su = vec2(sp.x / aspect, 1.0 - sp.y);

    vec3 light = vec3(0.0), streak = vec3(0.0);
    flash(sp, ROW(m0, 0), light, streak); flash(sp, ROW(m0, 1), light, streak);
    flash(sp, ROW(m0, 2), light, streak); flash(sp, ROW(m0, 3), light, streak);
    flash(sp, ROW(m1, 0), light, streak); flash(sp, ROW(m1, 1), light, streak);
    flash(sp, ROW(m1, 2), light, streak); flash(sp, ROW(m1, 3), light, streak);
    flash(sp, ROW(m2, 0), light, streak); flash(sp, ROW(m2, 1), light, streak);
    flash(sp, ROW(m2, 2), light, streak); flash(sp, ROW(m2, 3), light, streak);
    flash(sp, ROW(m3, 0), light, streak); flash(sp, ROW(m3, 1), light, streak);
    flash(sp, ROW(m3, 2), light, streak); flash(sp, ROW(m3, 3), light, streak);
    flash(sp, ROW(m4, 0), light, streak); flash(sp, ROW(m4, 1), light, streak);
    flash(sp, ROW(m4, 2), light, streak); flash(sp, ROW(m4, 3), light, streak);

    // the night: deep blue above, the city's haze below
    float up = clamp((sp.y - HORIZON) / (1.0 - HORIZON), 0.0, 1.0);
    vec3 c = mix(vec3(0.05, 0.03, 0.16) + tint.rgb * 0.03, vec3(0.004, 0.006, 0.04), pow(up, 0.55));
    c += tint.rgb * 0.07 * exp(-up * 7.0) + vec3(0.4, 0.1, 0.35) * 0.16 * exp(-up * 10.0);

    // clouds, dark until a burst lights them
    float n1 = noise(vec2(sp.x * 1.6 + time * 0.012, sp.y * 3.4));
    float n2 = noise(vec2(sp.x * 4.3 + time * 0.02, sp.y * 8.0 + 3.0));
    float n3 = noise(vec2(sp.x * 11.0 + time * 0.03, sp.y * 19.0));
    float cloud = smoothstep(0.42, 0.85, n1 * 0.6 + n2 * 0.28 + n3 * 0.12) * smoothstep(0.02, 0.3, up);

    // stars, twinkling, behind the clouds, paling when the sky is lit
    vec2 sg = sp * 85.0;
    float st = hash(floor(sg));
    if (st > 0.975) {
        vec2 f = fract(sg) - 0.5 - (vec2(hash(floor(sg) + 3.0), hash(floor(sg) + 7.0)) - 0.5) * 0.6;
        float tw = 0.45 + 0.55 * sin(time * (1.5 + 5.0 * fract(st * 91.0)) + st * 800.0);
        c += mix(vec3(0.6, 0.75, 1.0), vec3(1.0, 0.85, 0.7), fract(st * 37.0))
           * exp(-dot(f, f) * (30.0 + 60.0 * fract(st * 53.0))) * tw * up * (1.0 - cloud) / (1.0 + 6.0 * dot(light, vec3(0.33)));
    }
    c = mix(c, vec3(0.03, 0.025, 0.06) + tint.rgb * 0.03, cloud * 0.6);
    c += light * (0.4 + 1.2 * cloud * (0.4 + n3));

    // smoke, glowing with what burned in it and with what bursts now
    vec3 sm = texture(smoke, su, 1.0).rgb;
    float thick = max(sm.r, max(sm.g, sm.b));
    float puff = 0.45 + 1.1 * n2 * (0.5 + n3);
    c += sm * 0.2 * puff + thick * puff * light * 2.2;

    vec3 g = glow(su, soft) * (0.85 + 0.3 * level);

    // the city: a far row in the haze, a near row in front, its windows
    // lit as high as their part of the spectrum
    float roofFar = skyline(sp.x + 0.37, 11.0, 0.15, 21.0);
    if (sp.y < roofFar) {
        float dep = (roofFar - sp.y) / (roofFar - HORIZON);
        c = vec3(0.02, 0.016, 0.05) + tint.rgb * 0.05 + light * 0.3 * exp(-dep * 3.0);
        c += (light * 1.2 + vec3(0.03)) * smoothstep(0.003, 0.0, roofFar - sp.y);
    }
    float roof = skyline(sp.x, 26.0, 0.075, 3.0);
    if (sp.y < roof) {
        float dep = (roof - sp.y) / (roof - HORIZON);
        c = vec3(0.008, 0.008, 0.022) + tint.rgb * 0.015 + light * 0.3 * exp(-dep * 5.0);
        c += (light * 2.2 + vec3(0.05)) * smoothstep(0.004, 0.0, roof - sp.y);
        vec2 wn = vec2(sp.x * 234.0, sp.y * 190.0);
        vec2 wf = fract(wn);
        float lamp = step(0.35, wf.x) * step(0.4, wf.y) * step(0.004, roof - sp.y);
        float band = texture(spectrum, vec2(fract((floor(sp.x * 26.0) + 0.5) / (26.0 * aspect)) * 0.85, 0.375)).r;
        float on = step(0.84, hash(floor(wn)));
        float eq = step((sp.y - HORIZON) / 0.09, band * 1.1) * step(0.45, hash(floor(wn) + 5.0));
        c += vec3(1.0, 0.72, 0.35) * on * lamp * (0.3 + 0.15 * hash(floor(wn) + 1.0));
        c += mix(max(tint.rgb, vec3(0.15)), vec3(0.1, 0.7, 1.0), 0.4) * eq * lamp * (0.35 + 0.8 * beat);
    }
    c += g + streak;

    if (water) {
        // the mirror: darker with depth, broken into glints
        c = vec3(0.003, 0.007, 0.024) + tint.rgb * 0.035 + c * glint * (0.25 + 0.75 * exp(-d * 5.0)) * 1.2;
        c += (light * 0.4 + vec3(0.04, 0.03, 0.05)) * smoothstep(0.004, 0.0, d);

        // boats, riding the swell, a lamp at each masthead
        for (int i = 0; i < 3; i++) {
            float fi = float(i);
            float s = i == 0 ? 1.0 : (i == 1 ? 1.7 : 0.75);
            vec2 at = vec2((i == 0 ? 0.2 : (i == 1 ? 0.66 : 0.9)) * aspect,
                           HORIZON - (i == 0 ? 0.07 : (i == 1 ? 0.13 : 0.045)) + 0.002 * s * sin(time * 1.1 + fi * 2.0));
            vec2 q = p - at;
            if (abs(q.x) > 0.06 * s || q.y > 0.07 * s) continue;
            float lean = 0.04 * sin(time * 0.9 + fi * 1.7);
            float m = boat(vec2(q.x - q.y * lean, q.y), s);
            c = mix(c, vec3(0.004, 0.005, 0.012) + light * 0.06, m);
            vec2 l = q - vec2(0.062 * s * lean, 0.064 * s);
            c += vec3(1.0, 0.75, 0.4) * (exp(-dot(l, l) * 300000.0 / (s * s)) + 0.25 * exp(-dot(l, l) * 20000.0 / (s * s)));
            // and the lamp's shimmer under the hull
            float below = -q.y;
            if (below > 0.0)
                c += vec3(1.0, 0.7, 0.35) * 0.25 * exp(-below * 60.0 / s)
                   * exp(-pow((q.x + 0.004 * sin(below * 500.0 + time * 3.0)) * 500.0 / s, 2.0));
        }
    }

    c *= 1.0 + beat * 0.1;
    // a soft lens: darker corners; bright, never clipped flat; colors kept deep
    vec2 v = uv - 0.5;
    c *= 1.0 - 0.55 * dot(v, v);
    c = 1.0 - exp(-c * 1.5);
    c = clamp(mix(vec3(dot(c, vec3(0.3, 0.59, 0.11))), c, 1.25), 0.0, 1.0);
    fragColor = vec4(c, 1.0) * qt_Opacity;
}
