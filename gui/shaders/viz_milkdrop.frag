// Milkdrop on screen: the feedback texture over a deep glowing ground,
// bloomed from its own blurred mip levels, split a little into its colors
// toward the edges like light through a lens, and toned so the hot cores
// burn white while the trails keep their color.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float aspect;
    float beat;
    float bass;
    vec4 ground; // the deep background
    vec4 glow;   // the cover's color: the light behind it all
};
layout(binding = 1) uniform sampler2D feed;

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = (uv - 0.5) * vec2(aspect, 1.0);
    float r = length(p);

    // the ground: deep, lit from the middle, brighter on the kick
    vec3 col = ground.rgb * (1.0 - 0.5 * r);
    col += glow.rgb * (0.10 + 0.22 * beat + 0.10 * bass) * exp(-r * 3.0);

    // the light, its colors parted toward the edges
    vec2 off = (uv - 0.5) * 0.006 * (1.0 + 2.0 * beat);
    vec3 c = vec3(texture(feed, uv + off).r, texture(feed, uv).g, texture(feed, uv - off).b);
    // bloom from the blurred levels: a near glow and a wide one
    vec3 bloom = texture(feed, uv, 2.0).rgb * 0.5 + texture(feed, uv, 4.0).rgb * 0.6;
    vec3 light = c * 1.3 + bloom * (0.6 + 0.6 * beat);

    // tone: bright light runs toward white, gently, keeping its color
    light = 1.0 - exp(-light * 1.4);
    float hot = max(max(light.r, light.g), light.b);
    light = mix(light, vec3(hot), smoothstep(0.8, 1.0, hot) * 0.5);
    col = col * (1.0 - 0.7 * hot) + light;

    // darker toward the edges, as a screen is
    col *= 1.0 - 0.45 * dot(uv - 0.5, uv - 0.5) * 2.0;
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
