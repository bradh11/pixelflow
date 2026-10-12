/*{
  "DESCRIPTION": "Panes of colored glass with dark leading between them, slowly shifting shape as the light behind them flickers.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "panes", "LABEL": "Panes across", "TYPE": "float", "MIN": 2.0, "MAX": 12.0, "DEFAULT": 5.0 },
    { "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "leading", "LABEL": "Leading", "TYPE": "float", "MIN": 0.0, "MAX": 0.4, "DEFAULT": 0.1 },
    { "NAME": "flicker", "LABEL": "Flicker", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.4 },
    { "NAME": "colors", "LABEL": "Colors", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 1.0 },
    { "NAME": "shift", "LABEL": "Color shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 }
  ]
}*/

// A number from 0 to 1 that looks random and is always the same for the same square (and
// `which`, to get more than one number a square).
float scatter(vec2 square, int which) {
    int n = int(square.x) * 7919 + int(square.y) * 104729 + which * 15485863;
    n = (n ^ (n >> 13)) * 60493;
    n = (n ^ (n >> 11)) * 19937;
    n = n ^ (n >> 15);
    return float(n & 65535) / 65535.0;
}

// A fully saturated color from round the color wheel: 0 and 1 are the same red.
vec3 wheel(float turn) {
    return clamp(abs(fract(turn + vec3(0.0, 0.6667, 0.3333)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

void main() {
    vec2 p = isf_FragNormCoord * RENDERSIZE / min(RENDERSIZE.x, RENDERSIZE.y) * panes;
    vec2 square = floor(p);
    vec2 within = p - square;
    float t = TIME * speed * 0.5;
    // Each square of a grid has one point that wanders about its middle. A pixel belongs to the
    // pane of the nearest point; how much nearer that is than the next one says how far the
    // pixel is from the pane's edge.
    float nearest = 8.0;
    float next = 8.0;
    vec2 pane = square;
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            vec2 step_to = vec2(float(i), float(j));
            vec2 other = square + step_to;
            float a = scatter(other, 1) * 6.2831853;
            float b = scatter(other, 2) * 6.2831853;
            vec2 point = step_to + 0.5 + 0.35 * vec2(sin(t + a), cos(t * 0.8 + b));
            float away = length(point - within);
            if (away < nearest) {
                next = nearest;
                nearest = away;
                pane = other;
            } else if (away < next) {
                next = away;
            }
        }
    }
    float glass = smoothstep(leading, leading + 0.08, next - nearest);
    float own = scatter(pane, 3);
    float lit = 1.0 - flicker * (0.5 + 0.5 * sin(TIME * speed * (0.6 + own * 1.7) + own * 40.0));
    gl_FragColor = vec4(wheel(shift + own * colors) * glass * lit, 1.0);
}
