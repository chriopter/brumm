// matrix, the rain's next state: run once a cell (the target is as small
// as the grid). A cell's light fades; a head falling through it lights it
// again and gives it a new glyph. Each column has a few drops, each on its
// own track down the screen; one falls only if, as it sets out, the
// column's band (bass in the middle) and the highs let it. That choice is
// kept in a row below the screen, a bit a drop. A kick drops a squall of
// heads from the top.
//   out: r how lit, g the glyph's seed, b 1 where a head is; in the row
//   below, g the drops' choices.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 grid;      // columns, rows on screen (one more below)
    float fall;     // how far the rain has fallen, in rows
    float fallPrev; // the same a frame ago
    float decay;    // how much light a cell keeps this frame
    float high;     // the highs 0–1
    float kick0;    // where the rain stood at the last kick
    float kick1;    // and at the one before
    float kicks;    // kicks so far
};
layout(binding = 1) uniform sampler2D prev;
layout(binding = 2) uniform sampler2D spectrum;

const int DROPS = 4;

vec3 hash3(vec3 p) {
    p = fract(p * vec3(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yxz + 33.33);
    return fract((p.xxy + p.yxx) * p.zyx);
}

vec4 at(float c, float r) { return texture(prev, vec2(c + 0.5, r + 0.5) / vec2(grid.x, grid.y + 1.0)); }

void main() {
    vec2 cell = floor(qt_TexCoord0 * vec2(grid.x, grid.y + 1.0));
    float c = cell.x, r = cell.y;
    vec4 was = at(c, r);
    float lit = max(was.r * decay - 1.0 / 255.0, 0.0); // a step more, so 8 bits still fade out
    float seed = was.g;
    float head = 0.0;

    // The column's band: bass in the middle, highs at the edges.
    float mid = (grid.x - 1.0) * 0.5;
    float e = texture(spectrum, vec2(abs(c - mid) / (mid + 0.5), 1.5 / 4.0)).r;
    float chance = clamp(0.04 + 0.9 * e * e + 0.3 * high, 0.0, 0.95);

    float kept = floor(at(c, grid.y).g * 16.0 + 0.5);
    float keep = 0.0;
    for (int k = 0; k < DROPS; k++) {
        float bit = exp2(float(k));
        vec3 h = hash3(vec3(c, float(k) * 7.0 + 1.0, 3.0));
        float sp = 0.6 + 0.8 * h.x;
        float len = grid.y * (1.3 + 1.2 * h.y);
        float pos = fall * sp + h.z * len;
        float prevPos = fallPrev * sp + h.z * len;
        float cyc = floor(pos / len);
        float y = pos - cyc * len - 3.0;
        float y0 = floor(prevPos / len) == cyc ? prevPos - cyc * len - 3.0 : -4.0;
        float hy = floor(y), hy0 = min(floor(y0), hy - 1.0);
        // Above the screen a drop makes up its mind; on it, it keeps it.
        bool alive = y < 0.0
            ? hash3(vec3(c, float(k), mod(cyc, 997.0))).x < chance
            : mod(floor(kept / bit), 2.0) > 0.5;
        if (alive) keep += bit;
        if (alive && r > hy0 && r <= hy && r >= 0.0) {
            lit = 1.0;
            if (r > floor(y0)) seed = hash3(vec3(c, r, fract(fall * 0.618) * 400.0)).x;
            if (r == hy) head = 1.0;
        }
    }
    if (r >= grid.y) {
        fragColor = vec4(0.0, keep / 16.0, 0.0, 1.0);
        return;
    }

    // The squalls of the last two kicks.
    float squall = (2.0 + grid.x / 24.0) * 3.0 / grid.x;
    for (int k = 0; k < 2; k++) {
        float at0 = k == 0 ? kick0 : kick1;
        vec3 h = hash3(vec3(c, kicks - float(k), 11.0));
        if (h.x > squall) continue;
        float sp = 0.8 + 0.8 * h.y;
        float y = (fall - at0) * sp - 3.0 * h.z;
        float y0 = (fallPrev - at0) * sp - 3.0 * h.z;
        float hy = floor(y), hy0 = floor(y0);
        if (r >= 0.0 && r <= hy && r > hy0 - (hy == hy0 ? 1.0 : 0.0)) {
            lit = 1.0;
            if (r > hy0) seed = hash3(vec3(c, r, fract(fall * 0.618) * 400.0 + 5.0)).x;
            if (r == hy) head = 1.0;
        }
    }

    fragColor = vec4(lit, seed, head, 1.0);
}
