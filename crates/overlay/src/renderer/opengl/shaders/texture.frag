#version 130
out vec4 FragColor;
  
in vec2 TexCoord;

uniform sampler2D tex;

void main()
{
    FragColor = vec4(texture(tex, TexCoord));
}
