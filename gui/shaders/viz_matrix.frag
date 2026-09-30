// matrix, as it shows: each cell's glyph from the strip, mirrored, lit
// by the rain's state — white-hot heads, trails from bright green down to
// deep sea-green — and bloomed with the state blurred (its smaller mips),
// the heads flared sideways like light in a lens. Behind, a second rain
// of smaller, dimmer glyphs read from the same state, so it falls slower,
// as if further off, and a ribbon of green light waving through the dark.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 grid;        // columns, rows
    float glyphCount; // glyphs in the strip
    float churn;      // how far the glyphs have mutated
    float lod;        // the strip's mip level that fits a cell
    float flash;      // 1 on a kick, decaying
    float time;       // seconds, standing still when the style does
    float level;      // loudness 0–1
    float aspect;     // width / height
    vec4 tint;        // the cover's color
};
layout(binding = 1) uniform sampler2D state;
layout(binding = 2) uniform sampler2D atlas;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

// glyph: how much of cell's glyph covers the point f (0–1 in the cell).
float glyph(vec2 cell, float seed, vec2 f, float blur) {
    float h = hash(cell + seed * 17.0);
    float g = floor(fract(h * 7.13 + floor(churn * (0.3 + h) + h * 9.0) * 0.618) * glyphCount);
    vec2 uv = vec2((g + 1.0 - f.x) / glyphCount, f.y);
    return textureLod(atlas, uv, blur).r;
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 tex = vec2(grid.x, grid.y + 1.0);      // the state has a row more
    vec2 suv = vec2(uv.x, uv.y * grid.y / tex.y); // uv in the state
    vec3 green = mix(vec3(0.1, 1.0, 0.3), tint.rgb, 0.18);
    vec3 mint = vec3(0.75, 1.0, 0.85);
    vec3 sea = mix(vec3(0.0, 0.4, 0.3), tint.rgb * 0.5, 0.25);

    // The ground: deep green-black, a ribbon of light waving through it.
    vec3 col = mix(vec3(0.0, 0.03, 0.02), vec3(0.0, 0.0, 0.01), uv.y);
    float x = uv.x * aspect;
    float wave = 0.58 + 0.06 * sin(x * 1.7 + time * 0.35) + 0.03 * sin(x * 3.1 - time * 0.5);
    float band = exp(-abs(uv.y - wave) * (22.0 - 10.0 * level));
    col += green * band * (0.07 + 0.12 * level) + mint * pow(band, 12.0) * 0.08;

    // The far rain: smaller cells, their state from mirrored columns.
    vec2 bg = uv * grid * 1.7 + vec2(0.0, -0.37);
    vec2 bcell = floor(bg);
    vec2 bs = vec2(grid.x - 1.0 - mod(bcell.x * 3.0, grid.x), mod(bcell.y, grid.y));
    vec4 bst = texture(state, (bs + 0.5) / tex);
    col += sea * 0.6 * bst.r * glyph(bcell + 91.0, bst.g, fract(bg), lod + 1.2);

    // The near rain.
    vec2 g = uv * grid;
    vec2 cell = floor(g);
    vec4 st = texture(state, (cell + 0.5) / tex);
    float lit = min(1.0, st.r + 0.35 * flash * step(0.06, st.r));
    float shape = glyph(cell, st.g, fract(g), lod);
    float aura = glyph(cell, st.g, fract(g), lod + 1.8);
    vec3 c = mix(sea, green, smoothstep(0.1, 0.7, lit)) * lit * 1.4;
    c = mix(c, mint * 1.6, smoothstep(0.85, 1.0, lit) * 0.5);
    c = mix(c, vec3(2.2, 2.4, 2.3), st.b);
    col = col * (1.0 - shape * min(1.0, lit * 4.0) * 0.8) + c * shape + c * aura * 0.9;

    // Bloom: the state blurred glows around the rain, the heads most.
    vec2 tx = 1.0 / tex;
    vec4 b1 = textureLod(state, suv, 1.0);
    vec4 b2 = (textureLod(state, suv + tx * vec2(1.5, 1.5), 2.5) + textureLod(state, suv + tx * vec2(-1.5, 1.5), 2.5)
             + textureLod(state, suv + tx * vec2(1.5, -1.5), 2.5) + textureLod(state, suv + tx * vec2(-1.5, -1.5), 2.5)) * 0.25;
    col += green * (b1.r * 0.1 + b2.r * 0.22) * (1.0 + 1.5 * flash);
    col += mix(green, mint, 0.6) * (b1.b * 0.4 + b2.b * 0.6);

    // Heads flare sideways, a thin line of light through each.
    float streak = 0.0;
    for (int i = 0; i < 4; i++) {
        float o = (0.5 + float(i * i) * 0.8) * tx.x;
        streak += (texture(state, vec2(suv.x + o, suv.y)).b + texture(state, vec2(suv.x - o, suv.y)).b) * exp(-float(i) * 0.7);
    }
    float thin = pow(1.0 - abs(fract(g.y) - 0.5) * 2.0, 4.0);
    col += mix(green, mint, 0.7) * streak * thin * 0.3;

    // Film-lit, shaded at the edges.
    col = 1.0 - exp(-col * 1.4);
    vec2 v = (uv - 0.5) * vec2(aspect, 1.0);
    col *= 1.0 - 0.4 * smoothstep(0.45, 1.15, length(v));
    col += (hash(uv * 911.0 + time) - 0.5) / 255.0;
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
