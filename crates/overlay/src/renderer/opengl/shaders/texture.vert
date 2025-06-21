#version 130

out vec2 TexCoord;

uniform vec4 rect;

void main()
{
    const vec2 VERTICES[4] = vec2[4](
        vec2(0.0, 1.0),
        vec2(0.0, 0.0),
        vec2(1.0, 1.0),
        vec2(1.0, 0.0)
    );

    vec2 pos = VERTICES[gl_VertexID];
    gl_Position = vec4(rect.xy + pos * rect.zw, 1.0, 1.0);
    TexCoord = pos;
}
