// A list fading out at its top and bottom edges instead of being cut.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float fadeTop;    // how much of the height fades at the top, 0–1
    float fadeBottom; // and at the bottom
};
layout(binding = 1) uniform sampler2D source;

void main() {
    float y = qt_TexCoord0.y;
    float a = smoothstep(0.0, max(fadeTop, 1e-4), y) * smoothstep(0.0, max(fadeBottom, 1e-4), 1.0 - y);
    fragColor = texture(source, qt_TexCoord0) * a * qt_Opacity;
}
