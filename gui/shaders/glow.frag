// A glow behind what is chosen, as the XMB lights it: brightest near its
// middle and falling off smoothly to nothing in every direction.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 tint;
    float strength;
    float cx; // where its middle is across, 0–1
};

void main() {
    vec2 p = qt_TexCoord0 - vec2(cx, 0.5);
    p.x /= p.x < 0.0 ? max(cx, 0.01) : max(1.0 - cx, 0.01); // the ellipse reaches both ends
    p.y *= 2.0;
    float r2 = dot(p, p);
    // a wide, soft falloff that is already nothing where the item ends
    float a = exp(-r2 * 2.4) * (1.0 - smoothstep(0.35, 1.0, sqrt(r2))) * strength;
    fragColor = vec4(tint.rgb * a, a) * qt_Opacity;
}
