#version 440
// The display-name effects' vertex stage (namefx.frag paints): Qt's own
// ShaderEffect vertex shader, baked with the same GLSL versions as the
// fragment stage. OpenGL ES links a program only when both stages share a
// version, and Qt's built-in one has no GLSL ES 300 variant to pair with
// the fragment's (which needs 300 es for textureLod and textureSize).

layout(location = 0) in vec4 qt_Vertex;
layout(location = 1) in vec2 qt_MultiTexCoord0;
layout(location = 0) out vec2 qt_TexCoord0;

// namefx.frag's block, of which this stage reads the matrix only.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float light;
    float em;
    float ch;
    float letters;
    float phase;
    float rise;
    float flicker;
    vec2 box;
    vec4 line;
    vec4 c1;
    vec4 c2;
    vec4 c3;
    vec4 c4;
    vec4 c5;
    vec4 c6;
};

void main()
{
    qt_TexCoord0 = qt_MultiTexCoord0;
    gl_Position = qt_Matrix * qt_Vertex;
}
