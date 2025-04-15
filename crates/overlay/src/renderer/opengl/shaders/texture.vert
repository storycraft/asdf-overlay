#version 330 core

layout (location = 0) in vec2 pos;
out vec2 TexCoord;

uniform vec4 rect;

void main()
{
    gl_Position = vec4(rect.xy + pos * rect.zw, 1.0, 1.0);
    TexCoord = pos;
}
