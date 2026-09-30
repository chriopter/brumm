// A cover's reflection on the floor: the image upside down, fading out.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float strength; // how bright the reflection starts
};
layout(binding = 1) uniform sampler2D source;

void main() {
    vec2 uv = vec2(qt_TexCoord0.x, 1.0 - qt_TexCoord0.y);
    float fade = strength * pow(1.0 - qt_TexCoord0.y, 2.2);
    fragColor = texture(source, uv) * fade * qt_Opacity;
}
