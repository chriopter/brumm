// Fire: an inferno over a floor of black glass (qml/viz/Fire.qml). The
// burning itself is kept at half size (viz_fire_sim.frag); here it is seen
// through shimmering hot air, torn fine, colored from deep red through
// gold to a blue-white core, and bloomed into the dark. Smoke hangs over
// it, lit from below; embers glow; the floor mirrors it all between
// cracks of lava that pulse with the bass; a strong kick sends a shock
// ring out from the middle. Every sixteen kicks the fire changes its gas:
// blue, purple, green.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;     // seconds, standing still when the fire does
    float aspect;   // width / height
    float rise2;    // how far the fine noise has risen
    float flare;    // a kick, eased
    float bass;
    float level;    // loudness
    float blastAge; // seconds since the last shock
    float blastPow; // and how strong it was
    float huesFrom; // the gas the fire burned: 0 fire, 1 blue, 2 green, 3 purple
    float huesTo;   // the gas it burns now
    float huesMix;  // how far it has changed, 0–1
    vec4 tint;      // the cover's color
};
layout(binding = 1) uniform sampler2D sim;

const float FLOOR = 0.2; // the floor's edge, in heights

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
vec2 hash2(vec2 p) { return fract(sin(vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)))) * 43758.5453); }

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    float y0 = mod(i.y, 256.0), y1 = mod(i.y + 1.0, 256.0);
    return mix(mix(hash(vec2(i.x, y0)), hash(vec2(i.x + 1.0, y0)), u.x),
               mix(hash(vec2(i.x, y1)), hash(vec2(i.x + 1.0, y1)), u.x), u.y);
}

// the colors of a gas: its deep glow, its body, its bright tips
vec3 c1, c2, c3;
void gas(float m, out vec3 a, out vec3 b, out vec3 c) {
    if (m < 0.5)      { a = vec3(1.0, 0.04, 0.02); b = vec3(1.0, 0.38, 0.0);  c = vec3(1.0, 0.85, 0.3); }
    else if (m < 1.5) { a = vec3(0.03, 0.1, 1.0);  b = vec3(0.0, 0.5, 1.0);   c = vec3(0.5, 0.95, 1.0); }
    else if (m < 2.5) { a = vec3(0.0, 0.7, 0.12);  b = vec3(0.35, 1.0, 0.05); c = vec3(0.9, 1.0, 0.5); }
    else              { a = vec3(0.5, 0.0, 1.0);   b = vec3(1.0, 0.08, 0.85); c = vec3(1.0, 0.65, 0.95); }
}

// the glow of heat h: deep color, body, bright tips, a blue-white core
vec3 ramp(float h) {
    return c1 * 0.5 * smoothstep(0.0, 0.3, h)
         + c2 * 0.55 * smoothstep(0.22, 0.6, h)
         + c3 * 0.45 * smoothstep(0.55, 0.9, h)
         + vec3(0.6, 0.8, 1.2) * smoothstep(0.88, 1.15, h);
}

void main() {
    vec2 uv = qt_TexCoord0;
    float x = uv.x * aspect;
    float h = 1.0 - uv.y - FLOOR; // height above the floor's edge

    vec3 a0, b0, d0, a1, b1, d1;
    gas(huesFrom, a0, b0, d0);
    gas(huesTo, a1, b1, d1);
    c1 = mix(a0, a1, huesMix); c2 = mix(b0, b1, huesMix); c3 = mix(d0, d1, huesMix);

    // the shock: a ring running out from the middle of the floor's edge,
    // bending what is seen through it; flat on the floor, a dome above
    vec2 from = vec2(x - 0.5 * aspect, h < 0.0 ? h * 3.5 : h);
    float far = length(from);
    float ring = blastPow * exp(-blastAge * 2.2) * exp(-pow((far - blastAge * 1.7) / 0.028, 2.0));
    vec2 push = from / max(far, 0.001) * ring * 0.03;

    vec3 col;
    if (h >= 0.0) {
        // hot air: the picture shimmers, more the louder it burns
        float w1 = noise(vec2(x * 13.0, h * 9.0 - rise2)) - 0.5;
        float w2 = noise(vec2(x * 9.0 + 7.0, h * 14.0 - rise2 * 0.7)) - 0.5;
        vec2 suv = uv + vec2(w1, w2) * 0.007 * (0.4 + level + flare) + vec2(-push.x / aspect, push.y);
        vec4 s = texture(sim, suv);
        vec4 near = texture(sim, suv, 3.0); // two widths of bloom
        vec4 wide = texture(sim, uv, 5.5);

        // torn fine: the half-size heat cut by small fast noise
        float fine = noise(vec2(x * 42.0 + w1 * 4.0, h * 24.0 - rise2 * 2.2));
        fine = 0.65 * fine + 0.35 * noise(vec2(x * 95.0 - w2 * 6.0, h * 50.0 - rise2 * 3.1));
        float heat = s.r + (fine - 0.5) * 1.7 * sqrt(s.r) * (1.15 - s.r) + near.r * 0.1;

        // the dark behind: never quite black, the fire's color low in it
        col = vec3(0.012, 0.004, 0.02) + tint.rgb * 0.025 + c1 * 0.05 * exp(-h * 3.0);
        col += ramp(heat) * (1.0 + 0.5 * flare);
        col += c1 * wide.r * 0.55 + c2 * near.r * 0.2 + c2 * wide.r * wide.r * 0.5;

        // smoke, thick and rolling, lit by the fire under it
        float sm = clamp((near.g * 0.5 + s.g * 0.7) * 1.5 * smoothstep(-0.3, 0.25, w2 + 0.5 * w1), 0.0, 1.0);
        vec3 lit = vec3(0.03, 0.026, 0.035) + (c1 * 0.5 + c2 * 0.5) * (wide.r * 1.3 + 0.1 * exp(-h * 2.0));
        col = mix(col, lit, sm * 0.8 * (1.0 - smoothstep(0.15, 0.5, heat)));

        // embers: sharp streaks and their glow
        col += (c3 + vec3(0.3)) * s.b * 3.0 + c2 * near.b * 0.5;
        col += c2 * ring * 0.2;
    } else {
        // the floor: plates of black glass running to the edge, lava between
        float t = -h / FLOOR; // 0 at the edge, 1 at the bottom of the window
        vec2 f = vec2((x - 0.5 * aspect) / (0.3 + t), 1.0 / (0.3 + t)) * vec2(2.6, 3.4);
        vec2 cell = floor(f), fr = fract(f);
        float f1 = 9.0, f2 = 9.0;
        vec2 plate = vec2(0.0);
        for (int j = -1; j <= 1; j++)
            for (int i = -1; i <= 1; i++) {
                vec2 o = vec2(float(i), float(j));
                vec2 hh = hash2(cell + o);
                vec2 dd = o + hh - fr;
                float dist = dot(dd, dd);
                if (dist < f1) { f2 = f1; f1 = dist; plate = hh; }
                else if (dist < f2) f2 = dist;
            }
        float edge = sqrt(f2) - sqrt(f1);
        float pulse = 0.35 + 1.5 * bass + 1.2 * flare + 0.25 * sin(time * 1.3 + plate.x * 20.0);
        float crack = exp(-edge * 9.0) * pulse;
        vec3 lava = c1 * crack * 0.9 + c2 * crack * crack * 0.9 + c3 * pow(crack, 4.0) * 0.5;

        // the mirror: the fire upside down, each plate tilted a little, blurred with depth
        vec2 ruv = vec2(uv.x + (plate.x - 0.5) * 0.012 * t - push.x / aspect,
                        1.0 - (FLOOR + t * FLOOR * (2.6 + plate.y * 0.5 * t)));
        vec4 m = texture(sim, ruv, 1.2 + t * 2.5);
        vec4 mw = texture(sim, ruv, 5.0);
        vec3 refl = ramp(m.r) + c1 * mw.r * 0.35 + c2 * m.b * 2.0;
        float gloss = (0.75 - 0.5 * t) * mix(1.0, smoothstep(0.02, 0.1, edge), smoothstep(0.0, 0.3, t));
        col = vec3(0.008, 0.004, 0.012) + refl * gloss + lava * smoothstep(0.0, 0.35, t);

        // the edge glows, and the shock runs across the floor
        col += (c2 + c3 * flare) * exp(-t * 16.0) * (0.5 + 0.6 * flare + 0.4 * level);
        col += (c3 + vec3(0.2)) * ring * 1.3;
    }

    // a shock lights everything for a moment; the corners fall dark
    col *= 1.0 + 0.7 * blastPow * exp(-blastAge * 7.0);
    vec2 v = uv - 0.5;
    col *= 1.0 - 0.55 * dot(v, v);

    // hot spots bloom toward white rather than clip
    col = 1.0 - exp(-col * 1.5);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
