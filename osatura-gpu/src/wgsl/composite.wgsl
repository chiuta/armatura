// OSATURA GPU renderer: layer composite with opacity, optional alpha mask and W3C blend modes (own code).
// Reads a copy of the destination and the layer, writes the result (no fixed-function blending).

struct Comp {
    mode: u32,
    has_mask: u32,
    invert: u32,
    pad: u32,
    opacity: f32,
    pad1: f32,
    pad2: f32,
    pad3: f32,
};

@group(0) @binding(0) var<uniform> c: Comp;
@group(0) @binding(1) var dst_tex: texture_2d<f32>;
@group(0) @binding(2) var src_tex: texture_2d<f32>;
@group(0) @binding(3) var mask_tex: texture_2d<f32>;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One oversized triangle covering the viewport.
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn hardlight(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    let lo = cb * (2.0 * cs);
    let t = 2.0 * cs - 1.0;
    let hi = cb + t - cb * t;
    return select(hi, lo, cs <= vec3<f32>(0.5));
}

fn softlight(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    let dd = select(sqrt(cb), ((16.0 * cb - 12.0) * cb + 4.0) * cb, cb <= vec3<f32>(0.25));
    let lo = cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    let hi = cb + (2.0 * cs - 1.0) * (dd - cb);
    return select(hi, lo, cs <= vec3<f32>(0.5));
}

fn dodge1(cb: f32, cs: f32) -> f32 {
    if (cb == 0.0) { return 0.0; }
    if (cs >= 1.0) { return 1.0; }
    return min(1.0, cb / (1.0 - cs));
}

fn burn1(cb: f32, cs: f32) -> f32 {
    if (cb >= 1.0) { return 1.0; }
    if (cs <= 0.0) { return 0.0; }
    return 1.0 - min(1.0, (1.0 - cb) / cs);
}

// Blend function B(cb, cs) on straight colours. Mode numbers follow the order of `Blend` in osatura-core.
fn blend_fn(mode: u32, cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    switch (mode) {
        case 1u: { return cb * cs; }
        case 2u: { return cb + cs - cb * cs; }
        case 3u: { return hardlight(cs, cb); }
        case 4u: { return min(cb, cs); }
        case 5u: { return max(cb, cs); }
        case 6u: { return vec3<f32>(dodge1(cb.x, cs.x), dodge1(cb.y, cs.y), dodge1(cb.z, cs.z)); }
        case 7u: { return vec3<f32>(burn1(cb.x, cs.x), burn1(cb.y, cs.y), burn1(cb.z, cs.z)); }
        case 8u: { return hardlight(cb, cs); }
        case 9u: { return softlight(cb, cs); }
        case 10u: { return abs(cb - cs); }
        case 11u: { return cb + cs - 2.0 * cb * cs; }
        default: { return cs; }
    }
}

@fragment
fn fs_comp(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let ip = vec2<i32>(frag.xy);
    let dst = textureLoad(dst_tex, ip, 0);
    var src = textureLoad(src_tex, ip, 0);
    var k = c.opacity;
    if (c.has_mask != 0u) {
        var m = textureLoad(mask_tex, ip, 0).a;
        if (c.invert != 0u) { m = 1.0 - m; }
        k = k * m;
    }
    src = src * k;
    if (c.mode == 0u) {
        return src + dst * (1.0 - src.a);
    }
    if (c.mode == 12u) {
        return min(src + dst, vec4<f32>(1.0));
    }
    let ab = dst.a;
    let a_s = src.a;
    var cb = vec3<f32>(0.0);
    if (ab > 0.0) { cb = dst.rgb / ab; }
    var cs = vec3<f32>(0.0);
    if (a_s > 0.0) { cs = src.rgb / a_s; }
    let b = blend_fn(c.mode, cb, cs);
    let rgb = (1.0 - ab) * src.rgb + (1.0 - a_s) * dst.rgb + a_s * ab * b;
    return vec4<f32>(rgb, a_s + ab - a_s * ab);
}
