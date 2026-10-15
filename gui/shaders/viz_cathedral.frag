// Cathedral, the church (qml/viz/Cathedral.qml): a dark nave, and the sun
// coming through the glass (viz_cathedral_glass.frag, in a texture with
// its mip levels). The window stands large in the end wall, deep in a
// moulded reveal under the bay's great arch, a pier on either side, the
// floor polished marble. Every point of the room is lit by tracing it back
// along the sunlight to the window: what glass it finds there is the light
// it gets, so the window is thrown in color onto the floor, blurred the
// further it falls. One shader, three parts: the church (part 0), drawn at
// half size; the air between, read along each ray at a third of the size (part 1),
// which gives the shafts; and the glass
// alone at full size, laid over the church so it stays sharp (part 2).
// Dust hangs in four sheets at different depths, the nearer ones large and
// out of focus, and sparkles only where the sun catches it. The camera
// drifts slowly, the sun crosses, clouds pass. A kick makes the sun swell,
// the bass thickens the haze, the highs make the dust glitter.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float tick;   // seconds it has run
    float pan;    // how far the camera has drifted
    float aspect; // width / height
    float sunx;   // where the sun stands: its light's sideways slant
    float sun;    // how strong it shines: clouds, kicks
    float haze;   // how thick the air is
    float sparkle;// how the dust glitters
    float beat;
    vec4 tint;    // the cover's color
    float part;   // 0 the church; 1 the air alone, at a third of the size;
                  // 2 the glass alone, sharp, laid over the church
};
layout(binding = 1) uniform sampler2D glass;
layout(binding = 2) uniform sampler2D air; // what the air pass gathered

const float WW = 5.0;        // the window's width in the room
const float WH = 5.0 * 1.55; // its height
const float SILL = 1.4;      // how high it starts
const float PX = 3.9;  // the arcade's piers stand at ±PX,
const float PZ0 = -1.4; // the first PZ0 from the wall, then one more PDZ on
const float PDZ = 3.4;
const float PR = 0.42;
const float VAULT = 12.6; // the vault's height

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}

float fbm(vec2 p) {
    return noise(p) * 0.62 + noise(p * 2.7 + 7.1) * 0.38;
}

// inside a pointed (equilateral) arch: half width w, the arch springing at
// y = spring, standing on y = foot; > 0 inside, in units
float arch(vec2 p, float w, float foot, float spring) {
    float side = w - abs(p.x);
    float body = p.y < spring ? side : 2.0 * w - length(vec2(abs(p.x) + w, p.y - spring));
    return min(body, p.y - foot);
}

// the glass's halo at uv, blurred lod levels, reaching past its edges
vec3 haloAt(vec2 uv, float lod, float reach) {
    vec2 o = max(-uv, uv - 1.0);
    return textureLod(glass, clamp(uv, 0.0, 1.0), lod).rgb * exp(-max(max(o.x, o.y), 0.0) * reach);
}

// where on the glass texture a point of the end wall is
vec2 onGlass(vec2 w) { return vec2((w.x + WW * 0.5) / WW, 1.0 - (w.y - SILL) / WH); }

// the glass at uv, blurred lod levels; nothing beyond its edges
vec4 glassAt(vec2 uv, float lod) {
    vec2 out2 = max(-uv, uv - 1.0);
    float inside = smoothstep(0.02, 0.0, max(out2.x, out2.y));
    if (inside <= 0.0) return vec4(0.0);
    return textureLod(glass, clamp(uv, 0.0, 1.0), lod) * inside;
}

// the sunlight, as it travels
vec3 sunDir() { return normalize(vec3(sunx, -1.35, -1.0)); }

// Does a pier stand between p and the window, along the light?
float shadow(vec3 p, vec3 l) {
    float s = p.z / l.z;
    vec2 a = p.xz, b = (p - l * s).xz;
    float lit = 1.0;
    for (int i = 0; i < 4; i++) {
        vec2 c = vec2(i < 2 ? -PX : PX, PZ0 - PDZ * float(i - (i < 2 ? 0 : 2)));
        vec2 ab = b - a;
        float h = clamp(dot(c - a, ab) / max(dot(ab, ab), 1e-4), 0.0, 1.0);
        lit *= smoothstep(PR - 0.02, PR + 0.06, length(a + ab * h - c));
    }
    return lit;
}

// The light that reaches p from the window: trace it back along the sun.
// soft blurs it further the further it has come.
vec3 lightAt(vec3 p, vec3 l, float soft) {
    float s = p.z / l.z; // how far back the window is
    vec3 q = p - l * s;
    vec4 g = glassAt(onGlass(q.xy), clamp(soft * s, 0.0, 6.0));
    return g.rgb * exp(-s * 0.06);
}

// a ray against the two piers: the nearest hit and its normal
float piers(vec3 ro, vec3 rd, out vec3 n) {
    float best = 1e9;
    n = vec3(0.0);
    for (int i = 0; i < 4; i++) {
        vec2 c = vec2(i < 2 ? -PX : PX, PZ0 - PDZ * float(i - (i < 2 ? 0 : 2)));
        vec2 o = ro.xz - c, d = rd.xz;
        float a = dot(d, d), b = dot(o, d), cc = dot(o, o) - PR * PR;
        float disc = b * b - a * cc;
        if (disc > 0.0) {
            float t = (-b - sqrt(disc)) / a;
            if (t > 0.0 && t < best) {
                best = t;
                vec2 hp = o + d * t;
                n = vec3(hp.x, 0.0, hp.y) / PR;
            }
        }
    }
    return best;
}

// ashlar: courses of blocks, each its own shade, mortar between
vec3 stone(vec2 p) {
    float row = floor(p.y / 0.42);
    vec2 b = vec2(p.x / 0.78 + (mod(row, 2.0) == 0.0 ? 0.0 : 0.5), p.y / 0.42);
    vec2 cell = floor(b), f = fract(b);
    float mortar = smoothstep(0.0, 0.035, f.x) * smoothstep(1.0, 0.965, f.x)
                 * smoothstep(0.0, 0.06, f.y) * smoothstep(1.0, 0.94, f.y);
    float tone = 0.75 + 0.35 * hash(cell);
    float grain = 0.75 + 0.5 * fbm(p * 6.0 + cell);
    return vec3(0.52, 0.47, 0.40) * tone * grain * (0.35 + 0.65 * mortar);
}

// The window blooms over everything: three widths of glow, tight round
// the lead, a soft halo, and a wide breath of its colors over the dark.
vec3 bloom(vec3 ro, vec3 rd, float sunLight) {
    if (rd.z <= 0.0) return vec3(0.0);
    vec2 uv = onGlass((ro + rd * (-ro.z / rd.z)).xy);
    vec3 b = haloAt(uv, 2.5, 30.0) * 0.18 + haloAt(uv, 4.5, 9.0) * 0.4 + haloAt(uv, 6.5, 3.0) * 0.55;
    return b * sunLight * (0.55 + 0.4 * beat);
}

// Light over-exposes toward white, warm in the shadows; the corners dim.
vec3 finish(vec3 col) {
    col = 1.0 - exp(-col * 1.15);
    col = pow(col, vec3(0.95, 0.98, 1.04));
    vec2 e = qt_TexCoord0 - 0.5;
    col *= 1.0 - 0.55 * dot(e, e) * 2.0;
    return col + (hash(qt_TexCoord0 * 733.0 + tick) - 0.5) / 255.0;
}

void main() {
    // A gentle two-minute sweep across the glass. The aim moves more than
    // the camera, so the pan is visible instead of keeping the window locked
    // to the same spot. Long, independent periods avoid abrupt turns.
    float t = pan;
    const float TAU = 6.28318530718;
    vec3 ro = vec3(0.45 * sin(t * TAU / 180.0), 2.6 + 0.10 * sin(t * TAU / 210.0 + 1.0), -13.0 + 0.35 * sin(t * TAU / 240.0 + 2.0));
    vec3 at = vec3(1.6 * sin(t * TAU / 120.0), 3.5 + 0.2 * sin(t * TAU / 160.0), 0.0);
    vec3 fw = normalize(at - ro);
    vec3 rt = normalize(cross(vec3(0, 1, 0), fw));
    vec3 up = cross(fw, rt);
    float roll = 0.004 * sin(t * TAU / 200.0);
    vec2 sp = (qt_TexCoord0 - 0.5) * vec2(aspect, -1.0);
    sp = mat2(cos(roll), -sin(roll), sin(roll), cos(roll)) * sp;
    float focal = 1.0;
    vec3 rd = normalize(fw * focal + rt * sp.x + up * sp.y);

    vec3 l = sunDir();
    float sunLight = sun * 2.6;

    // what the ray meets: the end wall, the floor, or a pier
    float tw = rd.z > 0.0 ? -ro.z / rd.z : 1e9;
    float tf = rd.y < 0.0 ? -ro.y / rd.y : 1e9;
    vec3 pn;
    float tp = piers(ro, rd, pn);
    float tv = rd.y > 0.0 ? (VAULT - ro.y) / rd.y : 1e9;
    float th = min(min(min(tw, tf), tp), tv);

    if (part > 0.5 && part < 1.5) {
        // the shafts: the light gathered along the ray through the air.
        // The sun's light is parallel, so every point of the ray traces
        // back to the window along it, and the ray's points land on the
        // glass as a straight line, read in even steps. What a beam
        // carries is the outline of its light, kept sharp, filled with
        // the glass's color evened out: smooth inside, changing only at
        // the edges, so the steps need no scatter and leave no grain. The
        // outline is read at the blur that matches the steps' spacing, so
        // nothing between two reads is skipped.
        const int STEPS = 96;
        const vec2 TEX = vec2(768.0, 1190.0); // the glass texture's size
        float far = min(th, 26.0);
        float dt = far / float(STEPS);
        vec3 p0 = ro, p1 = ro + rd * far;
        vec2 uv0 = onGlass((p0 - l * (p0.z / l.z)).xy);
        vec2 uv1 = onGlass((p1 - l * (p1.z / l.z)).xy);
        float spacing = length((uv1 - uv0) * TEX) / float(STEPS);
        float lod = clamp(log2(max(spacing, 1.0)) + 0.3, 0.0, 6.0);
        vec3 gather = vec3(0.0);
        for (int i = 0; i < STEPS; i++) {
            float f = (float(i) + 0.5) / float(STEPS);
            vec3 p = mix(p0, p1, f);
            vec2 uv = mix(uv0, uv1, f);
            float back = p.z / l.z;
            // the air: a slow, wide swell of haze, no grain
            float dens = 0.8 + 0.2 * sin(p.x * 0.6 + tick * 0.17) * sin(p.y * 0.45 - tick * 0.11 + p.z * 0.3);
            float open = glassAt(uv, lod).a;        // the light's outline
            vec4 even = glassAt(uv, max(lod, 4.0)); // its glass, evened out
            gather += even.rgb / max(even.a, 0.08) * open * exp(-0.06 * back) * dens;
        }
        fragColor = vec4(gather * dt * 0.75, 1.0);
        return;
    }

    if (part > 1.5) {
        // the glass alone, at full size over the church drawn at half:
        // what it is sharp for. Elsewhere nothing.
        vec4 g = th == tw ? glassAt(onGlass((ro + rd * tw).xy), 0.0) : vec4(0.0);
        if (g.a < 0.004) { fragColor = vec4(0.0); return; }
        vec3 p = ro + rd * tw;
        vec3 seen = g.rgb * sunLight * 2.4 * (0.85 + 0.3 * fbm(p.xy * 0.6 + vec2(tick * 0.03, 0.0)));
        seen += texture(air, qt_TexCoord0).rgb * sunLight * (1.7 + 0.9 * haze);
        seen += bloom(ro, rd, sunLight);
        fragColor = vec4(finish(seen), 1.0) * g.a * qt_Opacity;
        return;
    }

    // the window's glow, whatever stands in front: its average color
    vec3 avg = textureLod(glass, vec2(0.5), 9.0).rgb;
    vec3 ambient = vec3(0.022, 0.024, 0.034) + tint.rgb * 0.004;
    vec3 col = vec3(0.0);

    if (th > 1e8) {
        col = ambient * 0.5;
    } else if (th == tv) {
        // the vault: ribs crossing between the bays, the cells between
        // them dark, lit faintly from the window
        vec3 p = ro + rd * th;
        float bay = fract((p.z - PZ0) / PDZ);
        float rib = smoothstep(0.035, 0.0, abs(abs(p.x / PX) - abs(bay * 2.0 - 1.0)))
                  + smoothstep(0.02, 0.0, abs(bay - 0.5) * 0.5) + smoothstep(0.03, 0.0, abs(p.x) / PX * 0.5);
        vec3 avg = textureLod(glass, vec2(0.5), 9.0).rgb;
        col = stone(p.xz) * (ambient * 2.0 + avg * sunLight * 0.12 / (1.0 + p.z * p.z * 0.02)) * (0.5 + 1.2 * min(rib, 1.0));
        col *= smoothstep(-24.0, -4.0, p.z) * 0.8 + 0.2;
    } else {
        vec3 p = ro + rd * th;
        vec3 wc = vec3(0.0, SILL + WH * 0.5, 0.0); // the window's middle
        float dw = length(p - wc);
        vec3 glow = avg * sunLight * 0.9 / (1.0 + dw * dw * 0.22);
        if (th == tw) {
            vec2 uv = onGlass(p.xy);
            vec4 g = glassAt(uv, 0.0);
            vec3 st = stone(p.xy);
            // the stone of the wall and the tracery: lit by the glow,
            // brighter in the window's splay
            vec3 spill = haloAt(uv, 4.0, 6.0);
            col = st * (ambient * 3.0 + glow * 0.5 + spill * sunLight * 1.2);
            // the window sits deep in the wall: its reveal steps back in
            // three orders of moulding, each catching the colored light
            // that comes through, with a shadowed groove between
            vec2 pw = vec2(p.x / WW, (p.y - SILL) / WW);
            float d = -arch(pw, 0.5, 0.0, 1.55 - 0.866); // > 0 outside the opening
            if (d > 0.0 && d < 0.1) {
                float order = d / 0.033;
                float f = fract(order);
                float roll = 0.45 + 0.55 * sin(f * 3.14159);       // each order rounded
                float groove = smoothstep(0.0, 0.12, f) * smoothstep(1.0, 0.88, f);
                float deep = 1.0 - floor(order) / 3.0;             // the inner orders nearer the glass
                vec3 lit = haloAt(uv, 2.5, 10.0) * (1.6 + 1.6 * deep) + avg * 0.4;
                col = vec3(0.55, 0.50, 0.43) * (0.8 + 0.3 * fbm(p.xy * 9.0)) * roll * groove
                    * (ambient * 4.0 + lit * sunLight * 0.9);
            }
            // the bay's great arch between the piers, moulded, the window
            // under it
            float wa = arch(p.xy, PX - PR - 0.05, 0.0, 6.6);
            if (wa > -0.4 && wa < 0.05 && d > 0.1) {
                float m = 0.55 + 0.45 * cos(wa * 48.0);
                col *= mix(1.0, 0.4 + 1.3 * m, smoothstep(-0.4, -0.3, wa) * smoothstep(0.05, 0.0, wa));
            }
            if (wa < -0.4) col *= 0.6; // the wall above the arch, further from the light
            // the vault's darkness overhead
            col *= smoothstep(14.0, 9.5, p.y);
            // the glass itself, the sun behind it
            if (g.a > 0.01) {
                vec3 seen = g.rgb * sunLight * 2.4;
                // the sky's brightness rides on it, and clouds move behind
                seen *= 0.85 + 0.3 * fbm(p.xy * 0.6 + vec2(tick * 0.03, 0.0));
                col = mix(col, seen, g.a);
            }
        } else if (th == tf) {
            // marble, in squares set on the diagonal, veined
            vec2 fp = p.xz;
            vec2 d = mat2(0.7071, -0.7071, 0.7071, 0.7071) * fp / 0.9;
            vec2 cell = floor(d), f = fract(d);
            float check = mod(cell.x + cell.y, 2.0);
            float joint = smoothstep(0.0, 0.02, f.x) * smoothstep(1.0, 0.98, f.x) * smoothstep(0.0, 0.02, f.y) * smoothstep(1.0, 0.98, f.y);
            float vein = smoothstep(0.03, 0.0, abs(fbm(fp * 1.7 + cell * 3.0) - 0.5));
            vec3 marble = mix(vec3(0.42, 0.40, 0.37), vec3(0.16, 0.13, 0.12), check) * (0.85 + 0.3 * hash(cell)) + vein * 0.12;
            marble *= 0.4 + 0.6 * joint;
            // the window thrown onto it, softer for having come far
            vec3 pool = lightAt(p, l, 0.35) * shadow(p, l) * (-l.y);
            col = marble * (ambient * 2.0 + glow * 0.6) + pool * sunLight * (marble * 3.0 + 0.12);
            // and the window mirrored in the polish
            vec3 rr = reflect(rd, vec3(0, 1, 0));
            if (rr.z > 0.0) {
                vec3 m = p + rr * (-p.z / rr.z);
                vec4 g = glassAt(onGlass(m.xy), 2.0 + 0.3 * th);
                float fres = 0.08 + 0.35 * pow(1.0 - max(-rd.y, 0.0), 4.0);
                col += g.rgb * sunLight * 2.0 * fres * joint * (0.6 + 0.4 * check);
            }
        } else {
            // a pier: stone round, the colored light wrapping it
            vec3 st = stone(vec2(atan(pn.z, pn.x) * PR * 1.6, p.y));
            float nl = max(dot(pn, -l), 0.0);
            float facing = max(dot(pn, normalize(wc - p)), 0.0);
            vec3 lit = lightAt(p, l, 0.3) * nl * shadow(p, l);
            col = st * (ambient * 3.0 + glow * (0.3 + 0.7 * facing) + lit * sunLight * 1.6);
            col *= smoothstep(8.5, 4.5, p.y);
        }
    }

    // the shafts, from the air pass, softened a little more
    col += texture(air, qt_TexCoord0).rgb * sunLight * (1.7 + 0.9 * haze);

    // dust in four sheets, the near ones large and soft
    for (int i = 0; i < 4; i++) {
        float z = -0.6 - float(i) * 1.5;
        if (rd.z <= 0.0) break;
        float tz = (z - ro.z) / rd.z;
        if (tz <= 0.15 || tz > th) continue;
        vec3 p = ro + rd * tz;
        vec2 q = p.xy * (5.0 - float(i) * 0.4) + float(i) * 13.7;
        q += vec2(0.25 * sin(tick * 0.11 + float(i)), tick * (0.035 + 0.01 * float(i)));
        vec2 cell = floor(q), f = fract(q) - 0.5;
        float h = hash(cell + float(i) * 3.1);
        if (h > 0.55) continue;
        vec2 o = vec2(hash(cell + 1.7), hash(cell + 4.3)) - 0.5;
        o += 0.18 * vec2(sin(tick * (0.4 + h) + h * 30.0), cos(tick * (0.33 + h) + h * 17.0));
        float r = mix(0.025, 0.07, h / 0.55) * (1.0 + 3.0 / max(tz, 0.6));
        float soft = clamp(0.6 / tz, 0.0, 0.9);
        float mote = smoothstep(r, r * soft, length(f - o * 0.7));
        if (mote <= 0.0) continue;
        float twinkle = 0.55 + 0.45 * sin(tick * (1.5 + 4.0 * h) + h * 60.0);
        twinkle = mix(twinkle, pow(twinkle, 6.0) * 3.0, sparkle);
        vec3 lit = lightAt(p, l, 0.5) * shadow(p, l);
        col += (lit * 1.6 + vec3(0.02)) * mote * twinkle * sunLight * (1.0 - 0.6 * soft);
    }

    col += bloom(ro, rd, sunLight);
    fragColor = vec4(finish(col), 1.0) * qt_Opacity;
}
