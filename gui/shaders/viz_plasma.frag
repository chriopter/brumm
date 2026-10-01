// Plasma: sine fields (columns, rows, a diagonal, and two trains of rings
// whose middles lie beyond the screen's edge, so no ring has a visible
// source) index a copper ring of theme colors, and every kick sends a wide
// swell through them from one side. Drawn as molten glass: the field is a
// surface, lit from above by two moving highlights, with a line of light
// at each step, an oil film's rainbow on its slopes, and the colors
// glowing as if lit from within. Everything here is smooth: no edge, no
// pole, no crease.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;    // the flow, eased by loudness
    float aspect;  // width / height
    float hue;     // how far round the ring the colors have turned, 0–1
    float bass;    // eased
    float beat;
    float level;   // eased
    vec4 ripples;  // the four kick swells' ages, seconds (< 0: none)
    vec4 sources;  // the two ring trains' middles, off screen (xy, zw)
};
layout(binding = 1) uniform sampler2D ring;

// the field at p (in rows of a 40-row screen, as the terminal's)
float field(vec2 p) {
    float t = time;
    vec2 q = p * vec2(0.5, 1.0);
    float da = length(q - sources.xy), db = length(q - sources.zw);
    float v = sin(p.x * 0.08 + t) + sin(p.y * 0.19 - t * 1.3)
            + sin(p.x * 0.045 + p.y * 0.11 + t * 0.7)
            + (0.7 + 0.9 * bass) * (0.62 * sin(da * 0.33 - t * 1.7) + 0.5 * sin(db * 0.24 + t * 1.1));
    // the swells: wide soft fronts running in from the first middle,
    // rising and fading gently so nothing ever pops
    for (int i = 0; i < 4; i++) {
        float age = ripples[i];
        float e = (da - 24.0 - age * 58.0) / (8.0 + 5.0 * age);
        v += 0.85 * smoothstep(0.0, 0.25, age) * (1.0 - smoothstep(0.9, 2.0, age)) * exp(-e * e);
    }
    return v;
}

void main() {
    vec2 uv = qt_TexCoord0;
    // in terminal cells: 40 rows high, twice as many columns as it is wide
    vec2 p = (uv - 0.5) * vec2(aspect * 80.0, 40.0);

    float v = field(p);
    // the surface's slope, for the light
    float e = 0.35;
    vec2 g = vec2(field(p + vec2(e, 0.0)) - v, field(p + vec2(0.0, e)) - v) / e;
    vec3 n = normalize(vec3(-g * 5.0, 1.0));

    float slope = length(g);

    // the color: thirteen steps of the ring over the field's range; the
    // kick pulls red and blue a little apart along it
    float pos = (v + 5.0) * (13.0 / 8.0) / 10.0 + hue;
    float ca = 0.0025 + 0.011 * beat;
    vec3 col = vec3(texture(ring, vec2(pos + ca, 0.5)).r, texture(ring, vec2(pos, 0.5)).g,
                    texture(ring, vec2(pos - ca, 0.5)).b);

    // deep glass: the color steeped, darker where the surface falls away,
    // glowing from within where it is bright, flaring on the kick
    col = pow(col, vec3(1.6));
    float lum = dot(col, vec3(0.3, 0.55, 0.15));
    col *= 0.35 + 0.65 * n.z * n.z + 0.3 * beat;
    col += col * lum * (0.8 + 1.0 * beat);

    // a thin line of light at each step of the ring, as the terminal's
    // bands have edges; it fades out where the surface lies flat, where
    // it would swell into a blot or meet another in a corner
    float step10 = abs(fract(pos * 10.0) - 0.5) * 2.0;
    col += mix(col, vec3(1.0), 0.35) * pow(step10, 18.0) * (0.5 + 0.8 * beat)
         * smoothstep(0.012, 0.07, slope);

    // an oil film on the slopes: a faint rainbow where the surface turns
    // away, fuller when it is loud
    float away = 1.0 - n.z;
    vec3 film = 0.5 + 0.5 * cos(6.2831853 * (v * 0.22 + n.z * 1.4 + vec3(0.0, 0.33, 0.67)));
    col += film * away * away * (0.22 + 0.5 * level);

    // a light sweeping slowly over the glass: a hot highlight and a wide
    // gloss around it; and a second, softer one going the other way
    vec3 l = normalize(vec3(0.7 * sin(time * 0.23), 0.6 * cos(time * 0.31), 1.0));
    vec3 h = normalize(l + vec3(0.0, 0.0, 1.0));
    float nh = max(dot(n, h), 0.0);
    col += vec3(1.0) * pow(nh, 120.0) * 0.9;
    col += mix(col, vec3(1.0), 0.5) * pow(nh, 16.0) * 0.3;
    vec3 l2 = normalize(vec3(-0.8 * cos(time * 0.19), 0.7 * sin(time * 0.27), 0.8));
    float nh2 = max(dot(n, normalize(l2 + vec3(0.0, 0.0, 1.0))), 0.0);
    col += mix(col, vec3(1.0), 0.6) * (pow(nh2, 70.0) * 0.5 + pow(nh2, 10.0) * 0.12);

    // tone, and darker toward the edges, as a screen is
    col = 1.0 - exp(-col * 1.6);
    col *= 1.0 - 0.6 * dot(uv - 0.5, uv - 0.5);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
