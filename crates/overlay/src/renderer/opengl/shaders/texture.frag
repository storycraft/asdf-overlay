#version 330 core
out vec4 FragColor;
  
in vec2 TexCoord;

uniform sampler2D tex;

void main()
{
    uint rawPacked = floatBitsToUint(texture(tex, TexCoord).r);
    FragColor = vec4(
        float((rawPacked & 0xffu)) / 255.0,
	  	float((rawPacked >> 8) & 0xffu) / 255.0,
		float((rawPacked >> 16) & 0xffu) / 255.0,
		float(rawPacked >> 24) / 255.0
    );
}
