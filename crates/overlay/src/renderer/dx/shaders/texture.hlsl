struct vs_out
{
	float4 position : SV_POSITION;
	float2 texCoord : TEXCOORD;
};

cbuffer OverlayBuffer : register(b0)
{
	float4 rect;
}

vs_out vs_main(uint index: SV_VertexID)
{
	static const float2 VERTICES[4] = {
        float2(0.0, 1.0),
        float2(0.0, 0.0),
        float2(1.0, 1.0),
        float2(1.0, 0.0)
	};

	vs_out output;
	float2 pos = VERTICES[index];
	output.position = float4(rect.xy + rect.zw * pos, 0.0, 1.0);
	output.texCoord = pos;

	return output;
}

Texture2D overlay : register(t0);
SamplerState overlaySampler: register(s0);

float4 ps_main(vs_out input) : SV_TARGET
{
	return overlay.Sample(overlaySampler, input.texCoord);
}
