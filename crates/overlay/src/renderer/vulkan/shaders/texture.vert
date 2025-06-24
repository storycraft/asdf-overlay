#version 450

layout(location = 0) out vec2 TexCoord;

layout(push_constant) uniform constants
{
	vec4 rect;
} PushConstants;

void main()
{
    const vec2 VERTICES[4] = vec2[4](
        vec2(0.0, 1.0),
        vec2(0.0, 0.0),
        vec2(1.0, 1.0),
        vec2(1.0, 0.0)
    );

    vec2 pos = VERTICES[gl_VertexIndex];
    gl_Position = vec4(PushConstants.rect.xy + pos * PushConstants.rect.zw, 1.0, 1.0);
    TexCoord = pos;
}
