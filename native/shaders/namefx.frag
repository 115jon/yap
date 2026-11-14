#version 440
// The Yap display-name effects (src/styles/name-styles.css; every value is
// the native spec's, name-styles-spec.md). DisplayName.qml draws the name
// once into its own layer (`source`): white glyphs, padded by the effect's
// reach, or for Glitch each letter in its class colour (red, green, blue
// for the 1st, 2nd and 3rd of every three letters; black for what is not a
// letter, the ellipsis). This shader paints that layer: the fill through
// the glyphs, then the effect's strokes, drops and glows, as the web's
// background-clip, -webkit-text-stroke, filter and text-shadow stack them.
// It runs only when the scene draws; a still name re-renders its layer only
// when the name changes, and an animated one moves only uniforms.
//
// One variant per effect: CMakeLists.txt bakes this file once per MODE (0
// dither, 1 holo, 2 chrome, 3 pixel, 4 crt, 5 glitch, 6 ember, 7 sticker,
// 8 candlelit) into :/shaders/name-<effect>.frag.qsb, so each is small
// (one shader with every branch made Direct3D's compiler run out of stack).
//
// Lengths are logical pixels; `em` is the name's font size.

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    // 1 on a light ground: the deeper palettes and the 0.5px guard.
    float light;
    float em;
    // The width of "0" (1ch) and the name's length in letters.
    float ch;
    float letters;
    // Holo: the sweep (0-1 of a tile). Pixel: the loop (0-1), or < 0 at
    // rest. Ember: the rise (0-0.46) and the flicker (0.58-1).
    float phase;
    float rise;
    float flicker;
    // The layer's size, and the name's line box inside it (x, y, w, h).
    vec2 box;
    vec4 line;
    // The effect's colours (opaque): dither A B; chrome c1-c6; pixel A B
    // shadow; glitch A; sticker A B shadow; candlelit A.
    vec4 c1;
    vec4 c2;
    vec4 c3;
    vec4 c4;
    vec4 c5;
    vec4 c6;
};

layout(binding = 1) uniform sampler2D source;

const float BAYER8[64] = float[64](
    0.0, 32.0, 8.0, 40.0, 2.0, 34.0, 10.0, 42.0,
    48.0, 16.0, 56.0, 24.0, 50.0, 18.0, 58.0, 26.0,
    12.0, 44.0, 4.0, 36.0, 14.0, 46.0, 6.0, 38.0,
    60.0, 28.0, 52.0, 20.0, 62.0, 30.0, 54.0, 22.0,
    3.0, 35.0, 11.0, 43.0, 1.0, 33.0, 9.0, 41.0,
    51.0, 19.0, 59.0, 27.0, 49.0, 17.0, 57.0, 25.0,
    15.0, 47.0, 7.0, 39.0, 13.0, 45.0, 5.0, 37.0,
    63.0, 31.0, 55.0, 23.0, 61.0, 29.0, 53.0, 21.0);

vec2 at(vec2 p)
{
    return p / box;
}

// The glyphs' coverage at `p`.
float mask(vec2 p)
{
    return texture(source, at(p)).a;
}

// Glitch: the torn coverage at `p`. Each class keeps its letters above its
// cut and slides the part below by its tear (CSS: 46% +0.08em, 62%
// -0.06em, 34% +0.03em of the line box); what has no class stays put.
float torn(vec2 p)
{
    vec4 here = texture(source, at(p));
    float loose = max(here.a - here.r - here.g - here.b, 0.0);
    float top = line.y;
    float h = line.w;
    float r = p.y < top + 0.46 * h ? here.r : texture(source, at(p - vec2(0.08 * em, 0.0))).r;
    float g = p.y < top + 0.62 * h ? here.g : texture(source, at(p - vec2(-0.06 * em, 0.0))).g;
    float b = p.y < top + 0.34 * h ? here.b : texture(source, at(p - vec2(0.03 * em, 0.0))).b;
    return min(loose + r + g + b, 1.0);
}

float cover(vec2 p)
{
#if MODE == 5
    return torn(p);
#else
    return mask(p);
#endif
}

// A Gaussian blur of the coverage (sigma in px), over a 5x5 grid read from
// the layer's mip chain so a wide glow is not undersampled.
float blurred(vec2 p, float sigma)
{
    float texels = textureSize(source, 0).x / max(box.x, 1.0);
    float lod = log2(max(sigma * texels * 0.5, 1.0));
    float sum = 0.0;
    float weight = 0.0;
    for (int j = -2; j <= 2; ++j) {
        for (int i = -2; i <= 2; ++i) {
            vec2 d = vec2(float(i), float(j)) * sigma * 0.9;
            float w = exp(-dot(d, d) / (2.0 * sigma * sigma));
            sum += w * textureLod(source, at(p + d), lod).a;
            weight += w;
        }
    }
    return sum / weight;
}

// The coverage grown by `r` px (a round stroke's outer edge).
float grown(vec2 p, float r)
{
    float a = mask(p);
    for (int k = 0; k < 12; ++k) {
        float t = float(k) * 0.52359878;
        vec2 d = vec2(cos(t), sin(t)) * r;
        a = max(a, max(mask(p + d), mask(p + d * 0.5)));
    }
    return a;
}

// The coverage shrunk by `r` px.
float shrunk(vec2 p, float r)
{
    float a = mask(p);
    for (int k = 0; k < 12; ++k) {
        float t = float(k) * 0.52359878;
        vec2 d = vec2(cos(t), sin(t)) * r;
        a = min(a, min(mask(p + d), mask(p + d * 0.5)));
    }
    return a;
}

// Premultiplied `top` over `under`.
vec4 over(vec4 top, vec4 under)
{
    return top + under * (1.0 - top.a);
}

vec4 ink(vec3 rgb, float a)
{
    return vec4(rgb * a, a);
}

vec3 rgb8(float r, float g, float b)
{
    return vec3(r, g, b) / 255.0;
}

// Stops at even steps (Holo: seven, the last the first again).
vec3 holo(float t)
{
    vec3 dark[7] = vec3[7](rgb8(255.0, 155.0, 229.0), rgb8(185.0, 166.0, 255.0),
        rgb8(141.0, 233.0, 255.0), rgb8(169.0, 255.0, 208.0), rgb8(255.0, 240.0, 163.0),
        rgb8(255.0, 179.0, 199.0), rgb8(255.0, 155.0, 229.0));
    vec3 pale[7] = vec3[7](rgb8(192.0, 18.0, 122.0), rgb8(109.0, 63.0, 216.0),
        rgb8(11.0, 121.0, 168.0), rgb8(14.0, 138.0, 87.0), rgb8(168.0, 106.0, 0.0),
        rgb8(194.0, 24.0, 91.0), rgb8(192.0, 18.0, 122.0));
    float x = clamp(t, 0.0, 1.0) * 6.0;
    int i = int(min(floor(x), 5.0));
    float f = x - float(i);
    return light > 0.5 ? mix(pale[i], pale[i + 1], f) : mix(dark[i], dark[i + 1], f);
}

vec3 ember(float t)
{
    float stops[6] = float[6](0.0, 0.16, 0.34, 0.52, 0.72, 1.0);
    vec3 dark[6] = vec3[6](rgb8(255.0, 246.0, 204.0), rgb8(255.0, 212.0, 92.0),
        rgb8(255.0, 164.0, 31.0), rgb8(255.0, 106.0, 13.0), rgb8(224.0, 52.0, 11.0),
        rgb8(163.0, 24.0, 10.0));
    vec3 pale[6] = vec3[6](rgb8(194.0, 65.0, 12.0), rgb8(217.0, 72.0, 15.0),
        rgb8(194.0, 65.0, 12.0), rgb8(180.0, 48.0, 10.0), rgb8(154.0, 31.0, 8.0),
        rgb8(110.0, 18.0, 5.0));
    t = clamp(t, 0.0, 1.0);
    vec3 c = light > 0.5 ? pale[5] : dark[5];
    for (int i = 4; i >= 0; --i) {
        if (t <= stops[i + 1]) {
            float f = (t - stops[i]) / (stops[i + 1] - stops[i]);
            c = light > 0.5 ? mix(pale[i], pale[i + 1], f) : mix(dark[i], dark[i + 1], f);
        }
    }
    return c;
}

vec3 chrome(float t)
{
    if (t < 0.3)
        return mix(c1.rgb, c2.rgb, t / 0.3);
    if (t < 0.5)
        return mix(c2.rgb, c3.rgb, (t - 0.3) / 0.2);
    // The horizon: a hard stop at the middle.
    if (t < 0.66)
        return mix(c4.rgb, c5.rgb, (t - 0.5) / 0.16);
    return mix(c5.rgb, c6.rgb, (t - 0.66) / 0.34);
}

// Two drop-shadows on the coverage `cov` (CSS filter: drop-shadow(-d 0 0
// a) then drop-shadow(d 0 0 b), each casting what is drawn so far).
vec4 rgbSplit(vec4 base, vec2 p, float d, vec3 left, vec3 right, float alpha)
{
    float a0 = cover(p);
    float aRight = cover(p + vec2(d, 0.0));
    float aLeft = cover(p - vec2(d, 0.0));
    vec4 one = over(base, ink(left, alpha * aRight));
    float oneLeft = aLeft + alpha * a0 * (1.0 - aLeft);
    return over(one, ink(right, alpha * oneLeft));
}

void main()
{
    vec2 p = qt_TexCoord0 * box;
    float a = cover(p);
    vec4 color = vec4(0.0);
    bool guard = light > 0.5;

#if MODE == 0
    {
        // Dither fade: cols x 8 cells over the box, the second ink's share
        // ramping left to right through the 8x8 Bayer matrix; past the
        // tile, the second ink.
        float cols = clamp(floor(letters * 6.0 + 0.5), 24.0, 256.0);
        float cx = floor(p.x / (box.x / cols));
        float cy = mod(floor(p.y / max(letters * ch / cols, 0.5)), 8.0);
        float share = clamp(((cx + 0.5) / cols - 0.08) / 0.84, 0.0, 1.0);
        float limit = BAYER8[int(cy) * 8 + int(mod(cx, 8.0))];
        bool second = cx >= cols || share * 64.0 > limit + 0.5;
        color = ink(second ? c2.rgb : c1.rgb, a);
    }
#elif MODE == 1
    {
        // Holo: a 100deg gradient in a 6em tile, sliding one tile per loop.
        float w = 6.0 * em;
        vec2 q = vec2(mod(p.x - phase * w, w), p.y);
        vec2 dir = vec2(0.98480775, 0.17364818);
        float len = abs(w * dir.x) + abs(box.y * dir.y);
        float t = dot(q - vec2(w, box.y) * 0.5, dir) / len + 0.5;
        color = ink(holo(t), a);
    }
#elif MODE == 2
    {
        // Chrome: the six-stop band with its horizon, a hairline stroke on
        // the glyph edge, and a hard drop.
        float r = max(0.6, 0.028 * em) * 0.5;
        float ring = grown(p, r) - shrunk(p, r);
        vec4 stroke = light > 0.5 ? ink(vec3(0.0), 0.5 * ring) : ink(vec3(1.0), 0.4 * ring);
        vec4 base = over(stroke, ink(chrome(p.y / box.y), a));
        color = over(base, ink(vec3(0.0), 0.55 * mask(p - vec2(0.0, 0.04 * em))));
    }
#elif MODE == 3
    {
        // Pixel shine: the base, two hard bars of shine in a 1.6em band
        // (12 steps across the name in 60% of the loop, then off it), and a
        // hard drop.
        float band = 1.6 * em;
        float end = letters * ch + 0.4 * em;
        float left = 0.4 * em;
        if (phase >= 0.0)
            left = phase < 0.6 ? -band + (end + band) * floor(phase / 0.6 * 12.0) / 12.0 : end;
        float f = (p.x - left) / band;
        vec3 fill = c1.rgb;
        if (f >= 0.18 && f < 0.3)
            fill = mix(c1.rgb, c2.rgb, 0.6);
        else if (f >= 0.42 && f < 0.72)
            fill = c2.rgb;
        vec4 base = ink(fill, a);
        float drop = max(1.0, 0.08 * em);
        color = over(base, ink(c3.rgb, mask(p - vec2(0.0, drop))));
    }
#elif MODE == 4
    {
        // CRT: scanlines, the RGB split, a phosphor glow (dark only).
        float sl = max(1.0, floor(0.07 * em + 0.5));
        bool bright = mod(floor(p.y / sl), 2.0) < 0.5;
        vec3 fill = light > 0.5 ? (bright ? rgb8(10.0, 122.0, 60.0) : rgb8(18.0, 161.0, 80.0))
                                : (bright ? rgb8(107.0, 255.0, 163.0) : rgb8(31.0, 165.0, 87.0));
        vec4 base = ink(fill, a);
        if (light > 0.5) {
            color = rgbSplit(base, p, 0.04 * em, rgb8(224.0, 0.0, 77.0), rgb8(0.0, 137.0, 179.0), 0.4);
        } else {
            color = rgbSplit(base, p, 0.045 * em, rgb8(255.0, 50.0, 100.0), rgb8(60.0, 170.0, 255.0), 0.6);
            color = over(color, ink(rgb8(70.0, 255.0, 150.0), 0.4 * blurred(p, 0.16 * em)));
        }
    }
#elif MODE == 5
    {
        // Glitch: the torn letters in the base colour, then the RGB split.
        vec4 base = ink(c1.rgb, a);
        if (light > 0.5)
            color = rgbSplit(base, p, 0.045 * em, rgb8(224.0, 0.0, 77.0), rgb8(0.0, 137.0, 179.0), 0.42);
        else
            color = rgbSplit(base, p, 0.06 * em, rgb8(255.0, 0.0, 92.0), rgb8(0.0, 225.0, 255.0), 0.72);
    }
#elif MODE == 6
    {
        // Ember: a 150% tall flame gradient that rises and falls, and two
        // flickering glows (the second lifted 0.05em; dark only).
        float t = (p.y + 0.5 * box.y * rise) / (1.5 * box.y);
        vec4 base = ink(ember(t), a);
        float g1 = blurred(p, 0.1 * em);
        float a1 = (light > 0.5 ? 0.32 : 0.55) * flicker;
        color = over(base, ink(rgb8(255.0, 120.0, 20.0), a1 * g1));
        if (light < 0.5) {
            float g2 = blurred(p + vec2(0.0, 0.05 * em), 0.22 * em);
            color = over(color, ink(rgb8(255.0, 70.0, 0.0), 0.4 * flicker * g2));
        }
    }
#elif MODE == 7
    {
        // Sticker: a 0.15em outline under the fill (0.075em shows outside),
        // joined across letters, and a hard drop of both.
        float r = 0.075 * em;
        float outline = grown(p, r);
        vec4 base = over(ink(c1.rgb, a), ink(c2.rgb, outline));
        float drop = grown(p - vec2(0.05, 0.07) * em, r);
        color = over(base, ink(c3.rgb, drop));
        guard = false;
    }
#else
    {
        // Candlelit serif: the ink in two warm text-shadows (CSS blur radius
        // r is a Gaussian of r / 2).
        vec4 base = ink(c1.rgb, a);
        vec4 glow;
        if (light > 0.5) {
            vec4 near = ink(rgb8(255.0, 190.0, 110.0), 0.5 * blurred(p, 0.02 * em));
            vec4 far = ink(rgb8(190.0, 90.0, 20.0), 0.32 * blurred(p - vec2(0.0, 0.03 * em), 0.1 * em));
            glow = over(near, far);
        } else {
            vec4 near = ink(rgb8(255.0, 214.0, 150.0), 0.55 * blurred(p, 0.03 * em));
            vec4 far = ink(rgb8(255.0, 150.0, 60.0), 0.45 * blurred(p, 0.14 * em));
            glow = over(near, far);
        }
        color = over(base, glow);
    }
#endif

    if (guard) {
        // drop-shadow(0 0 0.5px rgba(0, 0, 0, 0.42)) after the effect's own.
        float g = 0.0;
        for (int j = -1; j <= 1; ++j)
            for (int i = -1; i <= 1; ++i)
                g += (i == 0 && j == 0 ? 0.25 : (i == 0 || j == 0 ? 0.125 : 0.0625))
                    * mask(p + vec2(float(i), float(j)) * 0.6);
        color = over(color, ink(vec3(0.0), 0.42 * g));
    }

    fragColor = color * qt_Opacity;
}
