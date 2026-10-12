/*{
  "DESCRIPTION": "Curtains of light that sway and drift, like the northern lights. Best on a tall prop or the whole house.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "curtains", "LABEL": "Curtains", "TYPE": "float", "MIN": 1.0, "MAX": 8.0, "DEFAULT": 3.0 },
    { "NAME": "sway", "LABEL": "Sway", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
    { "NAME": "low", "LABEL": "Low color", "TYPE": "color", "DEFAULT": [0.1, 1.0, 0.45, 1.0] },
    { "NAME": "high", "LABEL": "High color", "TYPE": "color", "DEFAULT": [0.6, 0.2, 1.0, 1.0] }
  ]
}*/

// How bright the curtains are at one place across the picture: three waves of different lengths
// drifting past one another, so the folds never quite repeat.
float folds(float x, float t) {
    float wide = sin(x + t * 0.6);
    float middle = sin(x * 2.3 - t * 0.37 + 1.7);
    float fine = sin(x * 4.1 + t * 0.91 + 4.2);
    return 0.5 + 0.5 * (wide * 0.55 + middle * 0.3 + fine * 0.15);
}

void main() {
    vec2 uv = isf_FragNormCoord;
    float t = TIME * speed;
    // The curtains lean one way and then the other as they go up.
    float lean = sway * 0.6 * sin(uv.y * 2.4 + t * 0.5);
    float x = (uv.x + lean) * curtains * 3.14159265;
    float light = folds(x, t);
    light = light * light;
    // Brightest in a band that rises and falls, fading toward the ground and the sky.
    float band = 0.55 + 0.2 * sin(t * 0.23 + uv.x * 2.0);
    float rise = smoothstep(0.0, band, uv.y);
    float fall = 1.0 - 0.8 * smoothstep(band, 1.05, uv.y);
    float height = uv.y + 0.15 * sin(x * 0.5 + t);
    vec3 color = mix(low.rgb, high.rgb, smoothstep(0.2, 0.95, height));
    gl_FragColor = vec4(color, clamp(light * rise * fall * 1.8, 0.0, 1.0));
}
