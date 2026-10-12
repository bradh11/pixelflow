/*{
  "DESCRIPTION": "Soft blobs of color that drift, meet, and merge, like a lava lamp. Clear between the blobs, so it sits well over another effect.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "blobs", "LABEL": "Blobs", "TYPE": "long", "VALUES": [3, 5, 8], "LABELS": ["Three", "Five", "Eight"], "DEFAULT": 5 },
    { "NAME": "size", "LABEL": "Size", "TYPE": "float", "MIN": 0.05, "MAX": 0.5, "DEFAULT": 0.2 },
    { "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "soft", "LABEL": "Soft edges", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.35 },
    { "NAME": "first", "LABEL": "First color", "TYPE": "color", "DEFAULT": [1.0, 0.2, 0.05, 1.0] },
    { "NAME": "second", "LABEL": "Second color", "TYPE": "color", "DEFAULT": [1.0, 0.8, 0.1, 1.0] }
  ]
}*/

void main() {
    // The shorter side of the picture is 1 across, so the blobs are round on any prop.
    vec2 shape = RENDERSIZE / min(RENDERSIZE.x, RENDERSIZE.y);
    vec2 p = isf_FragNormCoord * shape;
    float t = TIME * speed;
    // Each blob pulls on the pixels near it; where the pulls add up to enough, there is lava.
    float pull = 0.0;
    float second_pull = 0.0;
    for (int i = 0; i < 8; i++) {
        if (i >= blobs) break;
        float k = float(i);
        vec2 at = 0.5 + 0.42 * vec2(sin(t * (0.31 + 0.07 * k) + k * 2.4), cos(t * (0.23 + 0.05 * k) + k * 1.3));
        vec2 away = p - at * shape;
        float one = size * size / (dot(away, away) + 0.0004);
        pull += one;
        // Odd blobs are the second color; where two kinds meet, the colors run together.
        second_pull += one * mod(k, 2.0);
    }
    float edge = 0.02 + soft * 0.6;
    float lava = smoothstep(1.0 - edge, 1.0 + edge, pull);
    vec3 color = mix(first.rgb, second.rgb, clamp(second_pull / max(pull, 0.001), 0.0, 1.0));
    gl_FragColor = vec4(color, lava);
}
