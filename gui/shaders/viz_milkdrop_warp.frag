// Milkdrop's feedback, one step: last frame zoomed out, twisted, faded and
// turned a little along the color wheel, with this frame's rings of light
// drawn over it — the spectrum ring, the waveform ring inside it, and the
// kick's shockwave. Drawn at half size into a texture that feeds itself.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float aspect;  // width / height
    float steps;   // how many 30ths of a second this frame is
    float rot;     // the rings' turn, radians
    float twist;   // the echoes' turn per 30th, radians (its sign flips)
    float bass;
    float beat;
    float level;
    float live;
    float since;   // seconds since the last kick
    vec4 hueA;     // the spectrum ring's color
    vec4 hueB;     // the waveform ring's color
    vec4 flash;    // the shockwave's color
};
layout(binding = 1) uniform sampler2D prev;
layout(binding = 2) uniform sampler2D spectrum;
layout(binding = 3) uniform sampler2D wave;

const float TAU = 6.2831853;

// a turn of the color wheel around the gray axis
vec3 hueTurn(vec3 c, float a) {
    const vec3 k = vec3(0.57735);
    float cs = cos(a);
    return c * cs + cross(k, c) * sin(a) + k * dot(k, c) * (1.0 - cs);
}

// a line of light: a hot thin core in a wide soft halo
float line(float d, float w) {
    return exp(-d * d / (w * w)) * 0.8 + 0.2 * exp(-abs(d) / (w * 2.0));
}

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = (uv - 0.5) * vec2(aspect, 1.0);

    // Where this pixel was: nearer the middle, turned back, and swirled a
    // little by a slow liquid field, so the echoes flow outward.
    float zoom = pow(mix(0.95, 0.87, smoothstep(0.35, 0.9, beat)), steps);
    float a = twist * steps;
    vec2 q = mat2(cos(a), sin(a), -sin(a), cos(a)) * p * zoom;
    q += 0.0035 * steps * vec2(sin(q.y * 7.0 + time * 0.9), sin(q.x * 6.0 - time * 0.7)) * (0.4 + level);
    vec2 src = q / vec2(aspect, 1.0) + 0.5;
    vec3 old = texture(prev, src).rgb;
    old *= step(0.0, src.x) * step(src.x, 1.0) * step(0.0, src.y) * step(src.y, 1.0);
    // faded, with a floor so eight bits of light still reach black
    old = max(hueTurn(old, 0.07 * steps) * pow(0.84, steps) - 0.6 / 255.0, 0.0);

    float r = length(p);
    float ang = atan(p.y, p.x);

    // the spectrum ring: radius by band, mirrored so it is whole
    float u = fract((ang - rot) / TAU);
    float fold = 1.0 - abs(2.0 * u - 1.0);
    float b = texture(spectrum, vec2(0.02 + fold * 0.62, 1.5 / 4.0)).r;
    float r0 = 0.15 + 0.08 * bass + 0.04 * beat;
    float ring = line(r - (r0 + b * 0.3), 0.0035);

    // the waveform ring, turning the other way; a slow sine when paused
    float u2 = fract(-(ang + rot * 1.7) / TAU);
    float fold2 = 1.0 - abs(2.0 * u2 - 1.0);
    float w = (texture(wave, vec2(fold2, 0.5)).r - 0.5) * 1.6;
    float idle = 0.12 * sin(u2 * 6.0 * TAU + time * 2.0) * (0.3 + 2.0 * level);
    float r2 = r0 * 0.55 * (1.0 + mix(idle, w, live));
    float inner = line(r - r2, 0.003);

    // the kick's shockwave, just past the spectrum ring
    float shock = exp(-since * 18.0) * line(r - (r0 + 0.315 + since * 0.4), 0.004);

    // new light over the old, as the brighter of the two, so it never burns out
    vec3 col = max(old, hueA.rgb * ring + vec3(0.25) * ring * ring * ring);
    col = max(col, hueB.rgb * inner * 0.85 + vec3(0.2) * inner * inner * inner);
    col += flash.rgb * shock;
    fragColor = vec4(min(col, vec3(1.0)), 1.0);
}
