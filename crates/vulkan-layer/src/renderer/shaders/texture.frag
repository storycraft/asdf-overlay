#version 450
layout(location = 0) out vec4 FragColor;
  
layout(location = 0) in vec2 TexCoord;

layout(binding = 0) uniform sampler2D tex;

void main()
{
    FragColor = vec4(texture(tex, TexCoord));
}
