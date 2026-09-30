// Scope's trigger and gain, one pixel kept from frame to frame: where the
// trace starts in the wave (the first rising zero crossing after the
// signal dipped, so periodic sounds stand still) and how much it is
// magnified (eased toward filling the screen). Out: r the start in
// samples / 255, g and b the gain / 4 (high and low byte), a 1 when there
// is sound to trace.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float live; // 1 playing … 0 paused
    float ease; // how far the gain moves toward its target this frame
};
layout(binding = 1) uniform sampler2D prev;
layout(binding = 2) uniform sampler2D wave;
layout(binding = 3) uniform sampler2D spectrum;

const float N = 256.0; // samples in the wave
const int L = 170;     // samples shown: two thirds

float at(int i) { return texture(wave, vec2((float(i) + 0.5) / N, 0.5)).r * 2.0 - 1.0; }

void main() {
    float peak = 0.0;
    for (int i = 0; i < 256; i++)
        peak = max(peak, abs(at(i)));
    bool sound = live > 0.5 && peak > 0.005;

    float start = floor((N - float(L)) / 2.0);
    float shown = 0.0;
    if (sound) {
        bool armed = false;
        float last = at(0);
        for (int j = 1; j <= 256 - L; j++) {
            float v = at(j);
            if (v < -0.1 * peak) {
                armed = true;
            } else if (armed && last < 0.0 && v >= 0.0) {
                start = float(j);
                break;
            }
            last = v;
        }
        for (int i = 0; i < L; i++)
            shown = max(shown, abs(at(int(start) + i)));
    } else {
        // the made-up signal's height: its partials' amplitudes, capped
        float norm = 0.0;
        for (int k = 0; k < 7; k++)
            norm += texture(spectrum, vec2((float(k) + 0.5) / 7.0, 1.5 / 4.0)).r;
        shown = 0.6 * min(1.0, norm);
    }
    float target = shown > 0.02 ? min(4.0, 0.85 / shown) : 1.0;
    target *= 0.3 + 0.7 * live;

    vec4 p = texture(prev, vec2(0.5));
    float gain = (p.g + p.b / 255.0) * 4.0;
    gain = gain <= 0.0 ? target : gain + (target - gain) * ease;
    float g = clamp(gain / 4.0, 0.0, 1.0) * 255.0;
    fragColor = vec4(start / 255.0, floor(g) / 255.0, fract(g), sound ? 1.0 : 0.0);
}
