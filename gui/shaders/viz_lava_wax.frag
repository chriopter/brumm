// Lava, the wax (qml/viz/Lava.qml): the blobs in three dimensions, found by
// marching a ray through them — at most 24 steps, at half size. Eight balls
// rise and sink between a pool on the lamp's floor and one under its cap,
// each as big as its part of the spectrum and stretched by its own speed;
// four drops ride on the first four and are thrown off on a kick, a strand
// of wax between them. Out comes no color, only the shape: the normal in
// red and green, how thick the wax is behind the pixel in blue, and how
// much of the pixel it covers in alpha. viz_lava.frag lights it.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float phase;  // how far the balls have drifted
    float tick;   // seconds it has run
    float aspect; // width / height
    float swell;  // a kick, eased: every ball grows
    float split;  // a kick, slower: the drops fly off
    float bass;
    float high;
};
layout(binding = 1) uniform sampler2D spectrum;

// per ball: x, y and z frequency, and where on its path it starts
const vec4 path[8] = vec4[8](
    vec4(0.31, 0.43, 0.17, 0.0), vec4(0.23, 0.37, 0.29, 2.1),
    vec4(0.41, 0.29, 0.13, 4.0), vec4(0.19, 0.53, 0.23, 1.1),
    vec4(0.37, 0.21, 0.31, 5.5), vec4(0.27, 0.47, 0.11, 3.2),
    vec4(0.45, 0.33, 0.19, 0.7), vec4(0.15, 0.39, 0.27, 4.7));

vec4 ball[12];   // center and radius
float tall[12];  // how far it is stretched upward

float smin(float a, float b, float k) {
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

// how far p is from the wax
float wax(vec3 p) {
    // the pools: the floor's heaves with the bass, the cap's hangs in drips
    // (both fall away toward the back and the front, so each shows as a ridge)
    float mid = exp(-p.x * p.x * 1.6);
    float away = 0.55 * p.z * p.z;
    float d = p.y + 0.90 + away - 0.07 * sin(p.x * 2.3 + phase * 0.9) * cos(p.z * 3.0 + phase * 0.6) - (0.06 + 0.30 * bass) * mid;
    float top = 0.94 + away - p.y - 0.08 * sin(p.x * 3.1 - phase * 0.7) * sin(p.z * 2.5 + phase * 0.5);
    d = min(d, top) * 0.75;
    float k = 0.30 + 0.25 * swell;
    for (int i = 0; i < 12; i++) {
        vec3 q = p - ball[i].xyz;
        q.y /= tall[i];
        d = smin(d, length(q) - ball[i].w, k);
    }
    // its skin is never still, and the highs make it shiver
    d += 0.012 * sin(p.x * 5.0 + tick * 0.9) * sin(p.y * 4.0 - tick * 0.7)
       + high * 0.010 * sin(p.x * 31.0 + tick * 11.0) * sin(p.y * 29.0 - tick * 13.0) * sin(p.z * 27.0 + tick * 7.0);
    return d;
}

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 s = vec2((uv.x * 2.0 - 1.0) * aspect, 1.0 - uv.y * 2.0);

    // where the balls are now
    for (int i = 0; i < 8; i++) {
        vec4 b = path[i];
        float a = phase * b.y * 2.0 + b.w * 1.7;
        float band = texture(spectrum, vec2((float(i) / 7.0 * 63.0 + 0.5) / 64.0, 1.5 / 4.0)).r;
        ball[i] = vec4(aspect * 0.78 * sin(phase * b.x * 2.0 + b.w),
                       0.88 * sin(a),
                       0.36 * sin(phase * b.z * 2.0 + b.w * 2.3),
                       0.19 * (0.6 + 1.3 * band + 1.2 * swell));
        tall[i] = 1.0 + 0.38 * abs(cos(a)) + 0.4 * swell;
    }
    for (int i = 0; i < 4; i++) {
        float a = phase * 1.3 + float(i) * 1.9;
        float r = ball[i].w;
        ball[8 + i] = vec4(ball[i].xyz + vec3(cos(a), sin(a) * 0.8, sin(a * 0.7) * 0.4) * (r * 0.7 + 0.62 * split), r * 0.55);
        tall[8 + i] = 1.0;
    }

    // the ray, from an eye well in front of the lamp
    vec3 ro = vec3(0.0, 0.0, -3.4);
    vec3 rd = normalize(vec3(s, 3.4));
    float t = 2.6 / rd.z, end = 4.3 / rd.z;
    float d = 1.0;
    for (int i = 0; i < 24; i++) {
        d = wax(ro + rd * t);
        if (d < 0.003 || t > end) break;
        t += d;
    }
    float cover = smoothstep(0.050, 0.012, d) * step(t, end + 0.2);
    if (cover <= 0.0) {
        fragColor = vec4(0.5, 0.5, 0.0, 0.0);
        return;
    }

    vec3 p = ro + rd * t;
    vec2 e = vec2(0.012, 0.0);
    float d0 = wax(p);
    vec3 n = normalize(vec3(wax(p + e.xyy), wax(p + e.yxy), wax(p + e.yyx)) - d0);
    // how much wax lies behind the skin here
    float thick = clamp(-wax(p - n * 0.16) / 0.16, 0.0, 1.0);
    thick *= 1.0 - 0.35 * smoothstep(-0.4, 0.6, p.z); // the far ones dimmer
    float grain = (hash(uv * 913.0 + tick) - 0.5) / 255.0; // against banding
    fragColor = vec4(n.xy * 0.5 + 0.5 + grain, thick, cover);
}
