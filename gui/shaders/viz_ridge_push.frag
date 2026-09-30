// Ridge's memory, 128 points × 41 rows, drawn over itself each frame: row 0
// is the live line (bass in the middle, flat noisy edges), the rows below
// the lines frozen before it, oldest last. On a push every row moves down
// one and the live line is frozen into row 1.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float shift; // 1 on a push, else 0
    float seed;  // the edges' noise, new with each push
};
layout(binding = 1) uniform sampler2D prev;
layout(binding = 2) uniform sampler2D spectrum;

const float ROWS = 41.0;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

void main() {
    vec2 uv = qt_TexCoord0;
    float row = floor(uv.y * ROWS);
    float v;
    if (row < 0.5) {
        float d = abs(uv.x - 0.5) * 2.0;
        float e = clamp((d - 0.5) / 0.45, 0.0, 1.0);
        float env = 1.0 - e * e * (3.0 - 2.0 * e);
        float n = hash(vec2(floor(uv.x * 128.0), seed));
        float s = texture(spectrum, vec2(min(d / 0.95, 1.0) * 0.97 + 0.015, 0.375)).r;
        v = env * s * (0.8 + 0.4 * n) + 0.015 * n;
        v *= 0.8; // headroom: the noise lifts it past 1
    } else {
        v = texture(prev, vec2(uv.x, (row - shift + 0.5) / ROWS)).r;
    }
    fragColor = vec4(v, 0.0, 0.0, 1.0);
}
