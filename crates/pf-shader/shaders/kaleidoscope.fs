/*{
  "DESCRIPTION": "Bands of color folded into mirrored wedges that turn about the middle, like looking down a kaleidoscope. Best on a matrix, a spinner, or anything round.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "wedges", "LABEL": "Wedges", "TYPE": "long", "VALUES": [3, 4, 6, 8, 12], "LABELS": ["3", "4", "6", "8", "12"], "DEFAULT": 6 },
    { "NAME": "turn", "LABEL": "Turn", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.3 },
    { "NAME": "flow", "LABEL": "Flow", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "zoom", "LABEL": "Zoom", "TYPE": "float", "MIN": 0.5, "MAX": 4.0, "DEFAULT": 1.5 },
    { "NAME": "shift", "LABEL": "Color shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 }
  ]
}*/

// A fully saturated color from round the color wheel: 0 and 1 are the same red.
vec3 wheel(float turn_of_wheel) {
    return clamp(abs(fract(turn_of_wheel + vec3(0.0, 0.6667, 0.3333)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

void main() {
    vec2 p = (isf_FragNormCoord - 0.5) * RENDERSIZE / min(RENDERSIZE.x, RENDERSIZE.y);
    float far = length(p) * zoom;
    // Fold the angle into one wedge and its mirror image.
    float wedge = 6.2831853 / float(wedges);
    float angle = abs(mod(atan(p.y, p.x) + TIME * turn, wedge) - wedge * 0.5);
    vec2 q = far * vec2(cos(angle), sin(angle));
    // Three sets of bands crossing in the folded picture.
    float t = TIME * flow;
    float a = sin(q.x * 6.0 - t * 1.3);
    float b = sin(q.y * 9.0 + q.x * 3.0 + t * 0.7);
    float c = sin((q.x + q.y) * 4.0 - t);
    float bands = (a + b + c) / 3.0;
    vec3 color = wheel(shift + far * 0.35 + bands * 0.25 - TIME * 0.05);
    gl_FragColor = vec4(color * (0.15 + 0.85 * smoothstep(-0.5, 0.3, bands)), 1.0);
}
