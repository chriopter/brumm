// The window's ground, after the XMB: a sky of slow clouds in the color of
// what plays, a glow along the horizon, and ribbons of light drifting
// across it. Cheap to draw; drawn only when it moves (qml/Backdrop.qml).
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;    // seconds; the sky drifts with it
    float aspect;  // width / height
    float glowing; // 0–1+: how lit the sky is
    vec4 topColor; // the sky, top to bottom
    vec4 bottomColor;
    vec4 glow;     // the light's color
};

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}

float clouds(vec2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * noise(p);
        p = p * 2.03 + vec2(1.7, 9.2);
        a *= 0.5;
    }
    return v;
}

float ribbon(vec2 uv, float k, float t) {
    float x = uv.x * aspect;
    float y = 0.5 + 0.07 * sin(x * 1.3 + t * 0.21 + k * 1.7)
                   + 0.03 * sin(x * 3.1 - t * 0.17 + k * 2.9)
                   + 0.012 * sin(x * 7.0 + t * 0.35 + k);
    float width = 0.003 + 0.045 * (0.5 + 0.5 * sin(x * 0.9 + t * 0.12 + k * 3.0));
    return exp(-abs(uv.y - y) / width) * (0.55 + 0.45 * sin(x * 1.7 - t * 0.09 + k));
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec3 col = mix(topColor.rgb, bottomColor.rgb, smoothstep(0.0, 1.0, uv.y));

    // the clouds, drifting slowly sideways
    vec2 p = vec2(uv.x * aspect, uv.y) * 1.6 + vec2(time * 0.018, 0.0);
    float c = clouds(p + clouds(p * 0.7 + time * 0.01));
    col += glow.rgb * 0.11 * glowing * smoothstep(0.35, 0.95, c) * (0.4 + 0.6 * (1.0 - 2.0 * abs(uv.y - 0.5)));

    // the horizon's glow across the middle, and the ribbons along it
    col += glow.rgb * 0.10 * glowing * exp(-abs(uv.y - 0.5) * 7.0);
    float light = 0.0;
    for (int i = 0; i < 3; i++)
        light += ribbon(uv, float(i), time) * (i == 0 ? 1.0 : 0.5);
    col += glow.rgb * light * 0.26 * glowing;
    col += vec3(1.0) * pow(max(light - 0.9, 0.0), 2.0) * 0.07 * glowing;

    // darker toward the edges, as a screen is
    vec2 v = uv - 0.5;
    col *= 1.0 - 0.35 * dot(v, v);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
