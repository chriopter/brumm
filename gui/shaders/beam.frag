// A line of light: a crisp core and its glow, both fading out smoothly
// toward the ends, with no step anywhere.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 tint;
    float strength;
    float h; // the item's height in pixels; the core runs along its middle
    float glow; // how far the glow reaches, in pixels
};

void main() {
    float x = qt_TexCoord0.x;
    float along = smoothstep(0.0, 0.3, x) * smoothstep(0.0, 0.3, 1.0 - x);
    float d = abs((qt_TexCoord0.y - 0.5) * h);
    float core = clamp(1.0 - d / 0.9, 0.0, 1.0);
    float haze = exp(-d * d / max(glow * glow * 0.35, 0.01)) * 0.45;
    float a = clamp(core * 0.85 + haze, 0.0, 1.0) * along * strength;
    fragColor = vec4(tint.rgb * a, a) * qt_Opacity;
}
