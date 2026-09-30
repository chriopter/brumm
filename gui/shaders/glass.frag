// A button of glass, round or a pill: a clear body darker toward its
// middle, a rim lit by the light passing along it (fresnel), a sharp
// highlight across its top and the light it gathers glowing at its
// bottom (caustic). Lit, the body is filled with color. Drawn only when
// the button changes.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;    // in pixels
    float radius; // corner radius, in pixels
    float lit;    // 0 glass … 1 filled with tint
    float hot;    // 0 … 1 pointed at
    vec4 tint;    // the color of what plays
};

float box(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

void main() {
    vec2 p = (qt_TexCoord0 - 0.5) * size;
    float d = box(p, size * 0.5 - 1.0, radius);
    float inside = clamp(0.5 - d, 0.0, 1.0);            // antialiased edge
    float depth = clamp(-d / (min(size.x, size.y) * 0.5), 0.0, 1.0); // 0 at the rim, 1 deep inside
    vec2 n = p / (size * 0.5);                             // -1 … 1 across

    float fresnel = pow(1.0 - depth, 3.0);
    float spec = smoothstep(0.55, 0.0, length((n - vec2(0.0, -0.52)) * vec2(0.85, 2.6))) * step(n.y, 0.1);
    float caustic = exp(-length((n - vec2(0.0, 0.78)) * vec2(1.1, 3.2)) * 2.2);

    vec3 body;
    float a;
    if (lit > 0.5) {
        body = mix(tint.rgb * 1.35, tint.rgb * 0.62, qt_TexCoord0.y);
        body += tint.rgb * caustic * 0.5 + vec3(1.0) * fresnel * 0.18;
        a = 1.0;
    } else {
        body = mix(vec3(1.0), tint.rgb, 0.35) * (0.10 + 0.06 * hot) + tint.rgb * caustic * (0.28 + 0.25 * hot);
        body += vec3(1.0) * fresnel * (0.22 + 0.2 * hot);
        a = 0.16 + 0.5 * fresnel + 0.3 * caustic + 0.08 * hot;
    }
    body += vec3(1.0) * spec * (lit > 0.5 ? 0.22 : 0.4); // a filled orb shows its color more than its shine
    a = max(a, spec * 0.6);
    // a hairline rim
    float rim = smoothstep(1.6, 0.4, abs(d + 0.8));
    body = mix(body, vec3(1.0), rim * (lit > 0.5 ? 0.45 : 0.22));
    a = max(a, rim * 0.5);
    a *= inside;
    fragColor = vec4(body * a, a) * qt_Opacity;
}
