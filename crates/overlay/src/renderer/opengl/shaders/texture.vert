#version 330 core

layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aTexPos;

out vec2 TexCoord;

void main()
{
    gl_Position = vec4(aPos, 1.0, 1.0);
    TexCoord = aTexPos;
}
