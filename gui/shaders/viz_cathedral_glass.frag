// Cathedral, the glass (qml/viz/Cathedral.qml): the window alone, flat,
// as if taken out of the wall and laid on a light table. A pointed arch
// filled after one of three churches: two lancets under a rose (Chartres),
// three tall lights of medallions under roundels (Sainte-Chapelle), or a
// great rose over a row of small lights (an oculus). Every light is cut
// into pieces of glass held by lead: a blue ground, ruby and gold borders
// of beads, medallions with their figures in green, gold, white and
// purple. Each piece has its own tint, streaks and bubbles, darker grime
// along its lead, and glows a little brighter with its part of the
// spectrum. The rose is cut twelvefold, its pieces mirrored in every
// petal. Out comes the light the glass lets through in rgb (lead and
// grime black) and in alpha where the opening is: 1 for glass and lead, 0
// for the stone of the tracery. viz_cathedral.frag lights the church with
// it. A new window comes in over the old one, piece by piece.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float tick;   // seconds it has run
    float design; // the window (0–2)
    float old;    // the window it comes in over
    float mixing; // 0 → 1 as it comes in
    float glow;   // how much the spectrum lights the pieces
};
layout(binding = 1) uniform sampler2D spectrum;

const float PI = 3.14159265;
const float H = 1.55; // the window's height; it is 1 wide
// Same geometry as the TUI: the whole oculus clears the pointed arch.
const float OCULUS_Y = 0.93;
const float OCULUS_RADIUS = 0.42;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
vec2 hash2(vec2 p) {
    return fract(sin(vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)))) * 43758.5453);
}

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}

// inside a pointed (equilateral) arch: half width w, the arch springing at
// y = spring, standing on y = foot; > 0 inside, in units
float arch(vec2 p, float w, float foot, float spring) {
    float side = w - abs(p.x);
    float body = p.y < spring ? side : 2.0 * w - length(vec2(abs(p.x) + w, p.y - spring));
    return min(body, p.y - foot);
}

// The pieces: cells around jittered points, s per unit. x: how far the
// nearest lead is, y and z: the piece's own random numbers.
vec3 pieces(vec2 p, float s) {
    vec2 q = p * s, c = floor(q), f = fract(q);
    float d1 = 8.0, d2 = 8.0;
    vec2 id = vec2(0.0);
    for (int j = -1; j <= 1; j++)
        for (int i = -1; i <= 1; i++) {
            vec2 g = vec2(float(i), float(j));
            vec2 o = g + 0.15 + 0.7 * hash2(c + g) - f;
            float d = dot(o, o);
            if (d < d1) { d2 = d1; d1 = d; id = c + g; }
            else if (d < d2) d2 = d;
        }
    // the distance to the nearest edge, roughly, back in units
    float edge = (sqrt(d2) - sqrt(d1)) * 0.5 / s;
    return vec3(edge, hash(id + 17.0), hash(id + 41.0));
}

// the old glass's colors
const vec3 BLUE = vec3(0.06, 0.20, 0.78);
const vec3 DEEP = vec3(0.03, 0.08, 0.45);
const vec3 SKY = vec3(0.25, 0.55, 0.95);
const vec3 RUBY = vec3(0.85, 0.04, 0.10);
const vec3 WINE = vec3(0.50, 0.02, 0.18);
const vec3 GOLD = vec3(1.00, 0.68, 0.10);
const vec3 GREEN = vec3(0.10, 0.55, 0.22);
const vec3 WHITE = vec3(0.92, 0.88, 0.75);
const vec3 PURPLE = vec3(0.42, 0.10, 0.62);

vec3 ground(float r) { // the blue ground, with a few jewels in it
    return r < 0.50 ? BLUE : r < 0.85 ? DEEP : r < 0.92 ? SKY : r < 0.97 ? RUBY : GOLD;
}
vec3 figure(float r) { // inside a medallion: the figures and their ground
    return r < 0.32 ? RUBY : r < 0.47 ? GREEN : r < 0.62 ? GOLD : r < 0.75 ? WHITE : r < 0.87 ? PURPLE : SKY;
}

// The glass of one region at p: its color and how much a piece lets
// through (0 on the lead). kind picks the ground, s the size of the pieces.
vec4 glass(vec2 p, vec2 seed, float s, int kind, float rim) {
    vec3 pc = pieces(p + seed, s);
    float r = pc.y;
    vec3 c = kind == 0 ? ground(r) : kind == 1 ? figure(r)
           : kind == 2 ? (r < 0.5 ? RUBY : r < 0.8 ? GOLD : WHITE)  // beads
           : kind == 3 ? (r < 0.6 ? GOLD : WHITE)                   // rings
           : (r < 0.4 ? WINE : r < 0.7 ? RUBY : PURPLE);            // a rose's petals
    // no two pieces alike: tint and thickness
    c *= 0.70 + 0.55 * pc.z;
    c = mix(c, c.gbr, 0.08 * (r - 0.5));
    // streaks in the melt, along each piece's own grain
    float a = pc.z * PI;
    vec2 g = mat2(cos(a), -sin(a), sin(a), cos(a)) * p;
    c *= 0.82 + 0.3 * noise(vec2(g.x * 90.0, g.y * 7.0) + r * 50.0);
    // bubbles trapped in it
    vec2 bq = p * 140.0 + r * 13.0, bc = floor(bq);
    float bh = hash(bc);
    if (bh > 0.92) c *= 1.0 + 0.6 * smoothstep(0.22, 0.0, length(fract(bq) - 0.5 - 0.2 * (hash2(bc) - 0.5)));
    // painted lines on the figures, fired into the glass
    if (kind == 1) c *= mix(1.0, 0.25, smoothstep(0.47, 0.5, noise(p * 26.0 + seed * 3.0)) * smoothstep(0.55, 0.52, noise(p * 26.0 + seed * 3.0)));
    // its part of the spectrum
    float band = texture(spectrum, vec2(0.05 + 0.75 * fract(r * 7.13), 1.5 / 4.0)).r;
    c *= 1.0 + glow * band * 0.9;
    // lead, and the grime along it
    float lead = max(0.0022, rim);
    float through = smoothstep(lead, lead + 0.0025, pc.x);
    through *= 0.55 + 0.45 * smoothstep(lead, lead + 0.02, pc.x);
    return vec4(c, through);
}

// a pointed light of medallions: q relative to its foot, w its half width
vec4 lancet(vec2 q, float w, float tall, vec2 seed) {
    float lead = 0.0;
    // the border of beads along its edge, ruby and gold, then the ground
    float edge = w - abs(q.x);
    if (q.y > tall) edge = min(edge, 2.0 * w - length(vec2(abs(q.x) + w, q.y - tall)));
    if (edge < 0.032) {
        vec4 b = glass(q, seed + 3.0, 70.0, 2, 0.0);
        b.a *= smoothstep(0.0, 0.003, abs(edge - 0.018));  // the lead between the rows
        return b;
    }
    // iron saddle bars every so high, heavy leads across the light
    float bar = abs(fract(q.y / (w * 1.9) + 0.5) - 0.5) * w * 1.9;
    // the medallions, one between each pair of bars
    float m = (floor(q.y / (w * 1.9)) + 0.5) * w * 1.9;
    float d = length(vec2(q.x, q.y - m));
    float rad = w * 0.72;
    if (q.y < tall && d < rad) {
        if (d > rad - 0.016) return glass(q, seed + 7.0, 90.0, 3, 0.0) * vec4(1, 1, 1, smoothstep(0.0, 0.003, rad - d) * smoothstep(0.0, 0.003, d - rad + 0.016));
        return glass(q, seed + floor(q.y / (w * 1.9)), 60.0, 1, 0.0);
    }
    // in the arch's head: a small roundel of white and gold
    if (q.y > tall) {
        float hd = length(vec2(q.x, q.y - tall - w * 0.55));
        if (hd < w * 0.42) return glass(q, seed + 9.0, 60.0, hd < w * 0.2 ? 3 : 4, 0.0) * vec4(1, 1, 1, smoothstep(0.0, 0.003, abs(hd - w * 0.2)) * smoothstep(0.0, 0.003, w * 0.42 - hd));
    }
    vec4 g = glass(q, seed, 46.0, 0, 0.0);
    g.a *= smoothstep(0.0035, 0.006, bar);
    return g;
}

// a rose of twelve petals, d its center, rr its radius; alpha 0 on its stone
vec4 rose(vec2 p, vec2 d, float rr, vec2 seed) {
    vec2 v = (p - d) / rr;
    float r = length(v);
    if (r > 1.0) return vec4(0.0);
    // fold into one half petal: twelve petals, each mirrored
    float a = atan(v.y, v.x) + PI / 2.0;
    float sec = 2.0 * PI / 12.0;
    float k = floor(a / sec);
    float fa = abs(mod(a, sec) - sec * 0.5);
    vec2 f = r * vec2(cos(fa), sin(fa)); // the folded point
    // the stone: a rim, a ring round the eye, spokes between petals, the
    // cusps of the outer ring
    float stone = 0.0;
    stone = max(stone, smoothstep(0.955, 0.965, r));
    stone = max(stone, smoothstep(0.012, 0.0, abs(r - 0.27) - 0.018));
    float spoke = sin(sec * 0.5 - fa) * r; // distance to the spoke, in rose units
    if (r > 0.27 && r < 0.96) stone = max(stone, smoothstep(0.024, 0.016, spoke));
    stone = max(stone, smoothstep(0.012, 0.0, abs(r - 0.70) - 0.012));
    vec2 pc = vec2(0.82, 0.0);
    float oc = length(f - pc);
    if (r > 0.70) stone = max(stone, smoothstep(0.012, 0.0, abs(oc - 0.115) - 0.012));
    if (stone >= 1.0) return vec4(0.0);
    vec4 g;
    vec2 fp = f * rr;
    if (r < 0.27) {
        // the eye: concentric, gold and white round a ruby heart
        g = glass(fp, seed + 2.0, 60.0, r < 0.1 ? 4 : 3, 0.0);
        g.a *= smoothstep(0.0, 0.01, abs(r - 0.1));
    } else if (r < 0.70) {
        // the petals: a medallion each, on blue
        float pd = length(f - vec2(0.49, 0.0));
        g = pd < 0.16 ? glass(fp, seed + 5.0 + k * 0.0, 70.0, 1, 0.0) : glass(fp, seed + 4.0, 55.0, 0, 0.0);
        g.a *= smoothstep(0.0, 0.01, abs(pd - 0.16));
    } else {
        // the outer ring: little roundels of wine and ruby, blue between
        g = oc < 0.115 ? glass(fp, seed + 6.0, 80.0, 4, 0.0) : glass(fp, seed + 8.0, 70.0, 0, 0.0);
    }
    g.a *= 1.0 - stone;
    return vec4(g.rgb * g.a, 1.0);
}

// a whole window, p in window units (x −0.5…0.5, y 0…H from the sill)
vec4 window(vec2 p, int w) {
    float outer = arch(p, 0.5, 0.0, H - 0.866);
    if (outer < 0.0) return vec4(0.0);
    vec4 g = vec4(0.0);
    if (w == 0) {
        // Chartres: two lancets under a rose, in plate tracery
        float side = p.x < 0.0 ? -1.0 : 1.0;
        vec2 q = vec2(p.x - side * 0.215, p.y - 0.05);
        if (arch(q, 0.165, 0.0, 0.62) > 0.0) {
            g = lancet(q, 0.165, 0.62, vec2(side * 3.0, 1.0));
            return vec4(g.rgb * g.a, 1.0);
        }
        return rose(p, vec2(0.0, 1.17), 0.27, vec2(11.0, 2.0));
    } else if (w == 1) {
        // Sainte-Chapelle: three tall lights, the middle one the tallest,
        // under three roundels
        float i = clamp(floor((p.x + 0.5) / 0.333), 0.0, 2.0);
        float cx = -0.333 + i * 0.333;
        vec2 q = vec2(p.x - cx, p.y - 0.04);
        float tall = i == 1.0 ? 0.80 : 0.68;
        if (arch(q, 0.135, 0.0, tall) > 0.0) {
            g = lancet(q, 0.135, tall, vec2(i * 5.0, 7.0));
            return vec4(g.rgb * g.a, 1.0);
        }
        vec4 a = rose(p, vec2(-0.17, 1.235), 0.105, vec2(1.0, 9.0));
        if (a.a > 0.0) return a;
        a = rose(p, vec2(0.17, 1.235), 0.105, vec2(2.0, 9.0));
        if (a.a > 0.0) return a;
        return rose(p, vec2(0.0, 1.40), 0.075, vec2(3.0, 9.0));
    }
    // an oculus: a great rose over a row of five small lights
    vec4 a = rose(p, vec2(0.0, OCULUS_Y), OCULUS_RADIUS, vec2(21.0, 5.0));
    if (a.a > 0.0) return a;
    float i = clamp(floor((p.x + 0.5) / 0.2), 0.0, 4.0);
    vec2 q = vec2(p.x - (-0.4 + i * 0.2), p.y - 0.04);
    if (arch(q, 0.072, 0.0, 0.26) > 0.0) {
        g = lancet(q, 0.072, 0.26, vec2(i * 2.0, 13.0));
        return vec4(g.rgb * g.a, 1.0);
    }
    return vec4(0.0);
}

void main() {
    vec2 p = vec2(qt_TexCoord0.x - 0.5, (1.0 - qt_TexCoord0.y) * H);
    int now = int(design + 0.5);
    vec4 c = window(p, now);
    if (mixing < 1.0) {
        // the new window comes in piece by piece, from the sill up
        float order = hash(floor(p * 40.0)) * 0.35 + p.y / H * 0.65;
        if (order > mixing * 1.3 - 0.15) c = window(p, int(old + 0.5));
    }
    fragColor = c;
}
