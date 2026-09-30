// Wire's phosphor, one step: last frame's light fades, and this frame's
// lines burn in over it. Drawn at half size into a feedback texture.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float fade; // how much of last frame's light is left
};
layout(binding = 1) uniform sampler2D prev;
layout(binding = 2) uniform sampler2D lines;

void main() {
    // (less a step, so eight bits fade all the way out)
    float old = max(texture(prev, qt_TexCoord0).r * fade - 1.5 / 255.0, 0.0);
    float now = texture(lines, qt_TexCoord0).r;
    fragColor = vec4(max(old, now), 0.0, 0.0, 1.0);
}
