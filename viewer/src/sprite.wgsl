struct Screen { size: vec2<f32>, zoom: f32, debug: f32, off: vec2<f32> }; // logical px; screen = world * zoom + off
@group(0) @binding(0) var<uniform> screen: Screen;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var mask: texture_2d<f32>; // <atlas>_m team-colour mask (black if the atlas has none)

struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32>, @location(2) team: vec4<f32> };

@vertex
fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>, @location(3) team: vec4<f32>) -> Out {
    let p = (pos * screen.zoom + screen.off) / screen.size; // px, y down -> clip space
    var o: Out;
    o.pos = vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    // vertex colours are sRGB values (XAML brushes like #008800, bytes / 255) and the surface is sRGB: decode them so
    // they reach the screen unchanged (white / black / alpha are unaffected)
    let c = color.rgb;
    o.color = vec4<f32>(select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045)), color.a);
    o.team = team;
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    if (i.color.a == 0.0) { return vec4<f32>(i.color.rgb, 1.0); } // flat map colour, no texture
    let t = textureSample(atlas, samp, i.uv);
    let m = textureSample(mask, samp, i.uv);
    if (screen.debug > 0.0) { return vec4<f32>(m.rgb, t.a); } // SHC_MASK=1: show the mask
    // Unlit/TeamColour (DXBC from resources.assets, re-read by hand): mask.r = where the SpriteRenderer colour
    // (palette team colour) multiplies in; mask.g + _SpriteCutoff (team.a, chopFeet) cuts the feet off.
    let s = saturate(10.0 * (i.team.a + m.g - 1.0));
    let a = saturate(t.a - s * s * (3.0 - 2.0 * s));
    let rgb = mix(t.rgb, t.rgb * i.team.rgb, m.r);
    return vec4<f32>(rgb, a) * i.color;
}
