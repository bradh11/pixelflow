/*{
  "DESCRIPTION": "Rings spreading from three drops on a pond, crossing and making patterns where they meet. Move the first drop with the point.",
  "CREDIT": "Brad Haas",
  "INPUTS": [
    { "NAME": "drop", "LABEL": "First drop", "TYPE": "point2D", "MIN": [0.0, 0.0], "MAX": [1.0, 1.0], "DEFAULT": [0.3, 0.6] },
    { "NAME": "rings", "LABEL": "Rings", "TYPE": "float", "MIN": 1.0, "MAX": 12.0, "DEFAULT": 3.0 },
    { "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": -4.0, "MAX": 4.0, "DEFAULT": 1.0 },
    { "NAME": "crisp", "LABEL": "Crisp rings", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.4 },
    { "NAME": "water", "LABEL": "Water", "TYPE": "color", "DEFAULT": [0.0, 0.15, 0.6, 1.0] },
    { "NAME": "light", "LABEL": "Light", "TYPE": "color", "DEFAULT": [0.5, 0.95, 1.0, 1.0] }
  ]
}*/

// The height of the water at `p` from a drop at `at`: rings that travel outward and die away.
float rings_from(vec2 p, vec2 at, float t) {
    float far = length(p - at);
    return cos(6.2831853 * (far * rings - t)) / (1.0 + far * 1.5);
}

void main() {
    // The longer side of the picture is 1 across: the rings are round, and as many cross a
    // tall prop as a wide one.
    vec2 shape = RENDERSIZE / max(RENDERSIZE.x, RENDERSIZE.y);
    vec2 p = isf_FragNormCoord * shape;
    float t = TIME * speed;
    // The other two drops keep their places relative to the first, turned a third and two thirds
    // of the way round the middle of the picture.
    vec2 middle = vec2(0.5);
    vec2 arm = drop - middle;
    vec2 second = middle + vec2(-0.5 * arm.x - 0.866 * arm.y, 0.866 * arm.x - 0.5 * arm.y);
    vec2 third = middle + vec2(-0.5 * arm.x + 0.866 * arm.y, -0.866 * arm.x - 0.5 * arm.y);
    float height = rings_from(p, drop * shape, t)
        + rings_from(p, second * shape, t * 0.83 + 0.4)
        + rings_from(p, third * shape, t * 1.19 + 0.7);
    // Crests catch the light; troughs are the water's own color.
    float crest = smoothstep(crisp * 0.7 - 0.2, 1.0, height * 0.6 + 0.4);
    gl_FragColor = vec4(mix(water.rgb, light.rgb, crest), 1.0);
}
