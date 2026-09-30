// The cover wall fading out around the cover in its middle: across, at
// the wall's ends; up and down, as far above the cover as below it.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 wallSize; // the wall, in pixels
    vec2 fadeEnd;  // gone this far from the middle, across and up/down, in pixels
    vec2 fadeSpan; // fading over this much before that, across and up/down
};
layout(binding = 1) uniform sampler2D source;

void main() {
    vec2 d = abs(qt_TexCoord0 - 0.5) * wallSize;
    vec2 a = 1.0 - smoothstep(fadeEnd - fadeSpan, fadeEnd, d);
    fragColor = texture(source, qt_TexCoord0) * a.x * a.y * qt_Opacity;
}
