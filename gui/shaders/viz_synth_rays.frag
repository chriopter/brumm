// Synth, the sun's rays: what is bright near the sun in the world is
// smeared away from it, so its light falls through the haze in shafts
// that the ridges and the sun's own stripes cut. Sixteen taps, drawn at a
// quarter of the size.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float aspect; // width / height
    vec2 sun;     // where the sun stands in the picture
};
layout(binding = 1) uniform sampler2D scene;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 d = sun - uv;
    float jit = hash(uv * 917.0);
    vec3 acc = vec3(0.0);
    float wgt = 1.0;
    for (int i = 0; i < 16; i++) {
        vec2 p = uv + d * (float(i) + jit) / 16.0 * 0.92;
        vec3 c = texture(scene, p).rgb;
        // only the sun's neighborhood throws rays
        vec2 o = (p - sun) * vec2(aspect, 1.0);
        float near = exp(-dot(o, o) * 9.0);
        acc += max(c - 0.6, 0.0) * near * wgt;
        wgt *= 0.93;
    }
    fragColor = vec4(acc / 9.0, 1.0);
}
