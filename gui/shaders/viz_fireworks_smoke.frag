// Fireworks, the smoke: what the sparks leave behind. Each frame the smoke
// of the last one rises a little, drifts with the wind, curls and thins,
// and takes on some of the light of the sparks burning in it. Drawn at
// quarter size into a feedback texture (qml/viz/Fireworks.qml); the
// picture lights it again with every later burst.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float dt;
    float aspect;
};
layout(binding = 1) uniform sampler2D prev;
layout(binding = 2) uniform sampler2D sparks;

void main() {
    vec2 uv = qt_TexCoord0;
    float wind = 0.5 + 0.4 * sin(time * 0.13);
    vec2 curl = vec2(sin(uv.y * 11.0 + time * 0.31) + sin(uv.y * 23.0 - time * 0.2),
                     cos(uv.x * 9.0 * aspect - time * 0.27)) * 0.004;
    vec2 flow = (vec2(wind * 0.016 / aspect, -0.006) + curl) * dt;
    vec3 old = texture(prev, uv - flow).rgb;
    // thins by a part and by a floor, so 8 bits get to black
    old = max(old * exp(-dt * 0.3) - 0.0035, 0.0);
    vec3 lit = texture(sparks, uv, 3.0).rgb * 2.0;
    fragColor = vec4(min(old + lit * dt * 1.6, 1.0), 1.0);
}
