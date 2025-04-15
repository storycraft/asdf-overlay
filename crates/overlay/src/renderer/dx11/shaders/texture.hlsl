struct vs_in
{
	float2 position : POSITION;
};

struct vs_out
{
	float4 position : SV_POSITION;
	float2 texCoord : TEXCOORD0;
};

cbuffer OverlayBuffer : register(b0)
{
	float4 rect;
}

vs_out vs_main(vs_in input)
{
	vs_out output;
	output.position = float4(rect.xy + rect.zw * input.position, 0.0, 1.0);
	output.texCoord = input.position;

	return output;
}

Texture2D overlay : register(t0);
SamplerState overlaySampler
{
	Filter = MIN_MAG_MIP_POINT;
	AddressU = Wrap;
	AddressV = Wrap;
};

float4 ps_main(vs_out input) : SV_TARGET
{
	return overlay.Sample(overlaySampler, input.texCoord);
}
