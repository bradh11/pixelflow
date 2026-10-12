/*{
  "DESCRIPTION": "A rainbow flowing over the prop: across it, up it, out from the middle, or round it. The one to reach for on an outline, an arch, or a tree.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "path", "LABEL": "Flows", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4], "LABELS": ["Across", "Up", "Diagonally", "Out from the middle", "Round the middle"], "DEFAULT": 0 },
    { "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": -4.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "rainbows", "LABEL": "Rainbows", "TYPE": "float", "MIN": 0.25, "MAX": 6.0, "DEFAULT": 1.0 },
    { "NAME": "wave", "LABEL": "Wave", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
    { "NAME": "pastel", "LABEL": "Pastel", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 }
  ]
}*/

// A fully saturated color from round the color wheel: 0 and 1 are the same red.
vec3 wheel(float turn) {
    return clamp(abs(fract(turn + vec3(0.0, 0.6667, 0.3333)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

void main() {
    vec2 uv = isf_FragNormCoord;
    vec2 p = (uv - 0.5) * RENDERSIZE / min(RENDERSIZE.x, RENDERSIZE.y);
    // How far along the rainbow's path this pixel is, and how far to one side of it.
    float along = uv.x;
    float beside = uv.y;
    if (path == 1) {
        along = uv.y;
        beside = uv.x;
    } else if (path == 2) {
        along = (uv.x + uv.y) * 0.5;
        beside = uv.x - uv.y;
    } else if (path == 3) {
        along = length(p);
        beside = atan(p.y, p.x) / 6.2831853 * 3.0;
    } else if (path == 4) {
        along = atan(p.y, p.x) / 6.2831853;
        beside = length(p);
    }
    along += wave * 0.15 * sin(beside * 9.0 + TIME * speed * 1.7);
    vec3 color = wheel(along * rainbows - TIME * speed * 0.25);
    gl_FragColor = vec4(mix(color, vec3(1.0), pastel * 0.7), 1.0);
}
