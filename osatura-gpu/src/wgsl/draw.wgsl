// OSATURA GPU renderer: stencil-then-cover fill shader (own code).
// Pass 1 (stencil): vertices are fan/stroke triangles, the fragment writes nothing (colour mask is empty).
// Pass 2 (cover): a bounding quad; the stencil test keeps only covered pixels; this shader computes the paint.

struct Draw {
    size: vec2<f32>,        // target size in pixels
    kind: u32,              // 0 solid, 1 linear gradient, 2 radial gradient
    nstops: u32,
    inv_abcd: vec4<f32>,    // pixel -> local matrix (a, b, c, d)
    inv_t: vec4<f32>,       // tx, ty, unused, unused
    geom: vec4<f32>,        // linear: from.xy to.xy; radial: centre.xy radius unused
    stops_c: array<vec4<f32>, 16>, // straight RGBA
    stops_o: array<vec4<f32>, 16>, // .x = offset
};

@group(0) @binding(0) var<uniform> d: Draw;

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos.x / d.size.x * 2.0 - 1.0, 1.0 - pos.y / d.size.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_stencil() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

fn ramp(t: f32) -> vec4<f32> {
    let n = d.nstops;
    if (t <= d.stops_o[0].x) {
        return d.stops_c[0];
    }
    for (var i = 1u; i < n; i = i + 1u) {
        let o1 = d.stops_o[i].x;
        if (t <= o1) {
            let o0 = d.stops_o[i - 1u].x;
            var u = 1.0;
            if (o1 > o0) {
                u = (t - o0) / (o1 - o0);
            }
            return mix(d.stops_c[i - 1u], d.stops_c[i], u);
        }
    }
    return d.stops_c[n - 1u];
}

@fragment
fn fs_cover(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = frag.xy; // pixel centre
    let p = vec2<f32>(
        d.inv_abcd.x * px.x + d.inv_abcd.z * px.y + d.inv_t.x,
        d.inv_abcd.y * px.x + d.inv_abcd.w * px.y + d.inv_t.y,
    );
    var c = d.stops_c[0];
    if (d.kind == 1u) {
        let a = d.geom.xy;
        let v = d.geom.zw - a;
        c = ramp(clamp(dot(p - a, v) / dot(v, v), 0.0, 1.0));
    } else if (d.kind == 2u) {
        c = ramp(clamp(length(p - d.geom.xy) / d.geom.z, 0.0, 1.0));
    }
    // Gradients interpolate straight colour, then premultiply (same order as the CPU rasteriser).
    return vec4<f32>(c.rgb * c.a, c.a);
}
