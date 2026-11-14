// HDR (scRGB, R16G16B16A16_FLOAT) to SDR (sRGB-encoded BT.709, BGRA8) for a
// share of an HDR display. One full-target triangle; each pixel reads its
// own texel (no filtering). The curve and its constants are explained in
// src/tonemap.rs, the CPU reference this must match.

Texture2D<float4> frame : register(t0);

cbuffer ToneMap : register(b0)
{
    float scale; // 80 / SDR white nits
    float peak;  // display peak, relative to SDR white (>= 1)
    float knee;  // shoulder start, relative to SDR white (<= 1)
    float unused;
};

float4 vs_main(uint id : SV_VertexID) : SV_Position
{
    float2 uv = float2((id << 1) & 2, id & 2);
    return float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
}

float shoulder(float m)
{
    if (m >= peak)
        return 1.0;
    float range = 1.0 - knee;
    float t = (m - knee) / range;
    float w = (peak - knee) / range;
    return knee + range * (t * (1.0 + t / (w * w)) / (1.0 + t));
}

float3 srgb_oetf(float3 c)
{
    float3 lo = c * 12.92;
    float3 hi = 1.055 * pow(c, 1.0 / 2.4) - 0.055;
    return float3(c.r <= 0.0031308 ? lo.r : hi.r,
                  c.g <= 0.0031308 ? lo.g : hi.g,
                  c.b <= 0.0031308 ? lo.b : hi.b);
}

float4 ps_main(float4 pos : SV_Position) : SV_Target
{
    float3 rgb = max(frame.Load(int3(pos.xy, 0)).rgb * scale, 0.0);
    float m = max(rgb.r, max(rgb.g, rgb.b));
    if (m > knee)
        rgb *= shoulder(m) / m;
    return float4(srgb_oetf(saturate(rgb)), 1.0);
}
