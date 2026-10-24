// OSATURA GPU renderer: box-filter resolve of the supersampled canvas (own code).
struct Down {
    factor: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};
@group(0) @binding(0) var<uniform> u: Down;
@group(0) @binding(1) var src_tex: texture_2d<f32>;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_down(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let base = vec2<i32>(frag.xy) * i32(u.factor);
    var acc = vec4<f32>(0.0);
    for (var y = 0u; y < u.factor; y = y + 1u) {
        for (var x = 0u; x < u.factor; x = x + 1u) {
            acc = acc + textureLoad(src_tex, base + vec2<i32>(i32(x), i32(y)), 0);
        }
    }
    return acc / f32(u.factor * u.factor);
}
