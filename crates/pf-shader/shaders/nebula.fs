/*{
  "DESCRIPTION": "Slow, billowing clouds of color that fold into one another, like smoke or a nebula. Clear where the cloud is thin.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "size", "LABEL": "Cloud size", "TYPE": "float", "MIN": 0.5, "MAX": 6.0, "DEFAULT": 2.0 },
    { "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "swirl", "LABEL": "Swirl", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
    { "NAME": "cover", "LABEL": "Cover", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.6 },
    { "NAME": "deep", "LABEL": "Thin cloud", "TYPE": "color", "DEFAULT": [0.25, 0.0, 0.9, 1.0] },
    { "NAME": "glow", "LABEL": "Thick cloud", "TYPE": "color", "DEFAULT": [1.0, 0.25, 0.55, 1.0] },
    { "NAME": "core", "LABEL": "Thickest", "TYPE": "color", "DEFAULT": [1.0, 0.9, 0.6, 1.0] }
  ]
}*/

// A number from 0 to 1 that looks random and is always the same for the same square.
float scatter(vec2 square) {
    int n = int(square.x) * 7919 + int(square.y) * 104729 + 15485863;
    n = (n ^ (n >> 13)) * 60493;
    n = (n ^ (n >> 11)) * 19937;
    n = n ^ (n >> 15);
    return float(n & 65535) / 65535.0;
}

// The random numbers of the four squares round `p`, blended smoothly from one to the next.
float lumps(vec2 p) {
    vec2 square = floor(p);
    vec2 f = p - square;
    f = f * f * (3.0 - 2.0 * f);
    float below = mix(scatter(square), scatter(square + vec2(1.0, 0.0)), f.x);
    float above = mix(scatter(square + vec2(0.0, 1.0)), scatter(square + vec2(1.0, 1.0)), f.x);
    return mix(below, above, f.y);
}

// Lumps of four sizes laid over one another, each half the size and half the strength of the
// one before: 0 to about 1.
float cloud(vec2 p) {
    float sum = 0.0;
    float strength = 0.5;
    for (int i = 0; i < 4; i++) {
        sum += strength * lumps(p);
        p = p * 2.03 + vec2(37.0, 11.0);
        strength *= 0.5;
    }
    return sum;
}

void main() {
    // Kept well away from zero, where the squares would mirror each other.
    vec2 p = isf_FragNormCoord * RENDERSIZE / min(RENDERSIZE.x, RENDERSIZE.y) * size + 64.0;
    float t = TIME * speed;
    // One cloud pushes another about, which is what makes it billow.
    vec2 push = vec2(cloud(p + vec2(0.0, t * 0.2)), cloud(p + vec2(5.2 - t * 0.15, 1.3)));
    float thick = cloud(p + (push - 0.5) * swirl * 3.0 + vec2(t * 0.1, 0.0));
    float thin = 0.75 - cover * 0.55;
    vec3 color = mix(deep.rgb, glow.rgb, smoothstep(thin, thin + 0.25, thick));
    color = mix(color, core.rgb, smoothstep(thin + 0.2, thin + 0.4, thick));
    gl_FragColor = vec4(color, smoothstep(thin - 0.2, thin + 0.05, thick));
}
