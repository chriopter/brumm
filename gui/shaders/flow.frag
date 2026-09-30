// A cover in the coverflow and its reflection on the floor, in one pass:
// the art with rounded corners on top, below it the art upside down,
// fading out. shade darkens it as it stands farther off.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float side;       // the cover's side, in pixels
    float radius;     // its corners, in pixels
    float reflection; // the reflection's height, as a share of the side
    float gap;        // between the cover and its reflection, in pixels
    float strength;   // how bright the reflection starts
    float shade;      // 0 lit … 1 dark
};
layout(binding = 1) uniform sampler2D source;

float corners(vec2 p) { // p in pixels from the cover's middle
    vec2 q = abs(p) - vec2(side * 0.5) + radius;
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    return clamp(0.5 - d, 0.0, 1.0);
}

void main() {
    float h = side * (1.0 + reflection) + gap;
    vec2 px = vec2(qt_TexCoord0.x * side, qt_TexCoord0.y * h);
    vec4 c = vec4(0.0);
    if (px.y < side) {
        c = texture(source, px / side) * corners(px - side * 0.5);
    } else if (px.y > side + gap) {
        float ry = px.y - side - gap;
        vec2 uv = vec2(px.x / side, 1.0 - ry / side);
        float fade = strength * pow(max(0.0, 1.0 - ry / (reflection * side)), 2.2);
        c = texture(source, uv) * corners(vec2(px.x, side - ry) - side * 0.5) * fade;
    }
    c.rgb *= 1.0 - 0.55 * shade;
    fragColor = c * qt_Opacity;
}
