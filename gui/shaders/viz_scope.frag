// Scope on the screen: a dark tube behind curved glass, the graticule
// glowing faintly, the phosphor's afterglow (a blue flash that fades to
// green) bloomed from its mip levels, and the beam itself, traced again
// at full size, burning white.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float beat;
    float bass;
    vec2 px;     // the screen's size in pixels
    vec4 ground; // the dark behind it all
    vec4 flash;  // the phosphor, fresh
    vec4 glow;   // the phosphor, lingering
    vec4 halo;   // the cover's color
};
layout(binding = 1) uniform sampler2D phosphor;
layout(binding = 2) uniform sampler2D scan;
layout(binding = 3) uniform sampler2D wave;
layout(binding = 4) uniform sampler2D spectrum;

const float N = 256.0; // samples in the wave

// the trace's height at x (0–1), −1…1 before the gain: the wave from the
// trigger on, or a few partials whose amplitudes are the spectrum's
float signal(float x, vec4 s) {
    if (s.a > 0.5) {
        float start = floor(s.r * 255.0 + 0.5);
        return texture(wave, vec2((start + x * 169.0 + 0.5) / N, 0.5)).r * 2.0 - 1.0;
    }
    float y = 0.0, norm = 0.0, u = x * 6.2831853;
    for (int k = 0; k < 7; k++) {
        float fk = float(k);
        float a = texture(spectrum, vec2((fk + 0.5) / 7.0, 1.5 / 4.0)).r;
        norm += a;
        y += a * sin(u * exp2(fk) * 1.5 + time * 1.5 * (fk + 1.0));
    }
    return norm > 0.0 ? y * min(1.0, norm) / norm : 0.0;
}

// the distance from p to the segment a–b
float seg(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a, ap = p - a;
    return length(ap - ab * clamp(dot(ap, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0));
}

// the distance in pixels from uv to the trace, walked in steps of 2 pixels
float trace(vec2 uv) {
    vec4 s = texture(scan, vec2(0.5));
    float gain = (s.g + s.b / 255.0) * 4.0 * 0.46;
    float h = 2.0 / px.x;
    vec2 p = uv * px;
    float x = uv.x - 2.0 * h;
    vec2 a = vec2(x, 0.5 - signal(x, s) * gain) * px;
    float d = 1e9;
    for (int i = -1; i <= 2; i++) {
        x = uv.x + float(i) * h;
        vec2 b = vec2(x, 0.5 - signal(x, s) * gain) * px;
        d = min(d, seg(p, a, b));
        a = b;
    }
    return d;
}

// the graticule: ten by eight divisions of dotted lines, the axes ticked
float graticule(vec2 uv) {
    vec2 p = uv * px;
    vec2 cell = px / vec2(10.0, 8.0);
    vec2 g = abs(fract(p / cell + 0.5) - 0.5) * cell;          // pixels to the nearest line
    vec2 dots = step(0.5, fract(p / 4.0));                      // dotted along each line
    float lines = max((1.0 - smoothstep(0.3, 1.3, g.x)) * dots.y, (1.0 - smoothstep(0.3, 1.3, g.y)) * dots.x);
    vec2 c = abs(p - px * 0.5);
    vec2 tick = abs(fract(p / (cell / 5.0) + 0.5) - 0.5) * cell / 5.0;
    float ticks = max((1.0 - smoothstep(0.4, 1.4, c.y)) + (1.0 - smoothstep(0.4, 1.4, tick.x)) * step(c.y, 6.0),
                      (1.0 - smoothstep(0.4, 1.4, c.x)) + (1.0 - smoothstep(0.4, 1.4, tick.y)) * step(c.x, 6.0));
    return lines * 0.5 + min(ticks, 1.0) * 0.8;
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 c = uv - 0.5;
    c.x *= px.x / px.y;

    // the tube: dark, a little lighter in the middle, the corners in shade
    float vig = 1.0 - 0.9 * pow(length(c * vec2(0.8, 1.1)), 2.4);
    vec3 base = ground.rgb * (0.7 + 0.5 * vig);
    base += halo.rgb * exp(-dot(c, c) * 3.0) * (0.05 + 0.06 * bass);

    // the graticule, lit a little by the beam near it
    vec2 ph = texture(phosphor, uv).rg;
    vec2 near = texture(phosphor, uv, 4.0).rg;
    vec3 light = mix(glow.rgb, flash.rgb, 0.3) * graticule(uv) * (0.2 + 0.6 * near.g);

    // the afterglow and its bloom
    vec2 b1 = texture(phosphor, uv, 2.0).rg;
    vec2 b2 = texture(phosphor, uv, 4.5).rg;
    vec2 b3 = texture(phosphor, uv, 6.5).rg;
    light += flash.rgb * ph.r * 0.9 + glow.rgb * ph.g * 0.8;
    light += flash.rgb * (b1.r * 0.5 + b2.r * 0.7 + b3.r * 0.6) * (0.8 + 0.8 * beat);
    light += glow.rgb * (b1.g * 0.4 + b2.g * 0.6 + b3.g * 0.7);

    // the beam, sharp: white at its core, blue at its edge
    float w = 1.2 + 2.5 * beat + 0.002 * px.y;
    float d = trace(uv);
    light += mix(flash.rgb, vec3(1.0), 0.75) * exp(-d * d / (w * w)) * 1.6;
    light += flash.rgb * exp(-d / (w * 3.0)) * 0.35;

    // the glass: faint scanlines and a gloss across the top
    light *= 0.93 + 0.07 * sin(uv.y * px.y * 2.0944);
    float gloss = smoothstep(0.25, 0.0, length((uv - vec2(0.35, -0.1)) * vec2(0.9, 2.2)) - 0.3);
    vec3 col = base + 1.0 - exp(-light * 1.2);
    col += vec3(gloss * 0.05) * vig;
    fragColor = vec4(col * vig * 0.15 + col * 0.85, 1.0) * qt_Opacity;
}
