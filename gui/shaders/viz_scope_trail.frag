// Scope's phosphor, one step: two afterglows, a quick one (the flash) and
// a slow one (the green that lingers), fading, and the beam burning into
// both where it passes now. Drawn at half size into a feedback texture.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float beat;
    float quick; // how much of each afterglow is left from last frame
    float slow;
    vec2 px;     // the texture's size in pixels
};
layout(binding = 1) uniform sampler2D prev;
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

void main() {
    vec2 old = texture(prev, qt_TexCoord0).rg;
    // (less a step, so eight bits fade all the way out)
    old = max(old * vec2(quick, slow) - 1.5 / 255.0, 0.0);
    float w = 0.9 + 1.2 * beat;
    float beam = 1.0 - smoothstep(w - 0.6, w + 0.8, trace(qt_TexCoord0));
    fragColor = vec4(max(old, vec2(beam, beam * 0.85)), 0.0, 1.0);
}
