/*{
  "DESCRIPTION": "Flying down a tunnel of checkered rings that twist as they rush past. Move the far end with the point.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "end", "LABEL": "Far end", "TYPE": "point2D", "MIN": [0.0, 0.0], "MAX": [1.0, 1.0], "DEFAULT": [0.5, 0.5] },
    { "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": -4.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "rings", "LABEL": "Rings", "TYPE": "float", "MIN": 1.0, "MAX": 10.0, "DEFAULT": 3.0 },
    { "NAME": "spokes", "LABEL": "Spokes", "TYPE": "long", "VALUES": [0, 2, 3, 4, 6, 8], "LABELS": ["None", "2", "3", "4", "6", "8"], "DEFAULT": 4 },
    { "NAME": "twist", "LABEL": "Twist", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.5 },
    { "NAME": "first", "LABEL": "First color", "TYPE": "color", "DEFAULT": [0.0, 0.6, 1.0, 1.0] },
    { "NAME": "second", "LABEL": "Second color", "TYPE": "color", "DEFAULT": [1.0, 0.1, 0.6, 1.0] }
  ]
}*/

// How far `x` is into its stripe, as a soft-edged 0 or 1: one for each half of every whole number.
float stripe(float x) {
    return smoothstep(0.4, 0.6, abs(fract(x) - 0.5) * 2.0);
}

void main() {
    vec2 p = (isf_FragNormCoord - end) * RENDERSIZE / min(RENDERSIZE.x, RENDERSIZE.y);
    float far = length(p) + 0.02;
    // The nearer a pixel is to the far end, the further down the tunnel it looks.
    float depth = 0.3 / far;
    float about = atan(p.y, p.x) / 6.2831853;
    float ring = stripe(depth * rings * 0.5 + TIME * speed);
    float checks = ring;
    if (spokes > 0) {
        float spoke = stripe((about + depth * twist * 0.25) * float(spokes) + TIME * speed * 0.1);
        // One or the other, not both: a checkerboard wrapped round the tunnel.
        checks = ring + spoke - 2.0 * ring * spoke;
    }
    // The far end is lost in the dark.
    float light = smoothstep(0.03, 0.3, far);
    gl_FragColor = vec4(mix(first.rgb, second.rgb, checks) * light, 1.0);
}
