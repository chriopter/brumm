// Plasma: four interfering sine fields (columns, rows, a diagonal and rings
// from the middle) index a copper ring of theme colors, and every kick sends
// a ripple out through them. Drawn as molten glass: the field is a surface,
// lit from above by a moving highlight, with a line of light at each step,
// and the colors glow as if lit from within.
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
    vec4 ripples;  // the four kick ripples' ages, seconds (< 0: none)
};
layout(binding = 1) uniform sampler2D ring;

// the field at p (in rows of a 40-row screen, as the terminal's)
float field(vec2 p) {
    float t = time;
    float d = length(p * vec2(0.5, 1.0));
    float v = sin(p.x * 0.08 + t) + sin(p.y * 0.19 - t * 1.3)
            + sin(p.x * 0.045 + p.y * 0.11 + t * 0.7)
            + (0.7 + 0.9 * bass) * sin(d * 0.33 - t * 1.7);
    for (int i = 0; i < 4; i++) {
        float age = ripples[i];
        float e = (d - age * 30.0) * 0.25;
        float bump = max(1.0 - e * e, 0.0);
        v += step(0.0, age) * 0.9 * max(1.0 - age / 1.6, 0.0) * bump * bump;
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

    // the color: thirteen steps of the ring over the field's range
    float pos = (v + 5.0) * (13.0 / 8.0) / 10.0 + hue;
    vec3 col = texture(ring, vec2(pos, 0.5)).rgb;

    // deep glass: the color steeped, darker where the surface falls away,
    // glowing from within where it is bright, flaring on the kick
    col = pow(col, vec3(1.6));
    float lum = dot(col, vec3(0.3, 0.55, 0.15));
    col *= 0.35 + 0.65 * n.z * n.z + 0.3 * beat;
    col += col * lum * (0.8 + 1.0 * beat);

    // a thin line of light at each step of the ring, as the terminal's
    // bands have edges
    float step10 = abs(fract(pos * 10.0) - 0.5) * 2.0;
    col += mix(col, vec3(1.0), 0.35) * pow(step10, 18.0) * (0.5 + 0.8 * beat);

    // a light sweeping slowly over the glass: a hot highlight and a wide
    // gloss around it
    vec3 l = normalize(vec3(0.7 * sin(time * 0.23), 0.6 * cos(time * 0.31), 1.0));
    vec3 h = normalize(l + vec3(0.0, 0.0, 1.0));
    float nh = max(dot(n, h), 0.0);
    col += vec3(1.0) * pow(nh, 120.0) * 0.9;
    col += mix(col, vec3(1.0), 0.5) * pow(nh, 16.0) * 0.3;

    // tone, and darker toward the edges, as a screen is
    col = 1.0 - exp(-col * 1.6);
    col *= 1.0 - 0.6 * dot(uv - 0.5, uv - 0.5);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
