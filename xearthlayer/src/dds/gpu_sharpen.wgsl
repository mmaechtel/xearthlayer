// texture.sharpen on the GPU worker's level 0 (`gpu_sharpen.rs`): the unsharp
// mask of `dds/sharpen.rs`, its operations in its order, in two render passes.
// `blur` writes each row's horizontal blur of the luma into an r32float
// target; `finish` blurs that vertically and writes the sharpened level 0.
// Every constant comes from `sharpen.rs` through `params`.

struct Params {
    // The image's own size (level 0 unpadded): the border and its ramp are
    // measured from its edges, as on the CPU.
    width: u32,
    height: u32,
    border: u32,
    ramp: u32,
    // Always zero: see `rounded`.
    zero: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    // `factors[step - 1]`: `Sharpen::ramp_factor(step)`.
    factors: vec4<f32>,
    // Rec. 709 luma weights in `xyz`.
    luma: vec4<f32>,
    // The Gaussian, one weight per element in `x`.
    kernel: array<vec4<f32>, 7>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var level0: texture_2d<f32>;
@group(0) @binding(2) var blurred: texture_2d<f32>;

// `x` unchanged, through an XOR with a zero the compiler cannot see. Every
// product the CPU adds goes through here, so it is rounded to f32 before the
// add: WGSL cannot forbid contraction, and a driver that fuses a multiply
// and an add into an FMA rounds once where the CPU rounds twice (RADV did,
// in the luma and both blurs, and texels came out one step off).
// The order of the additions is left as written: WGSL also lets a compiler
// reassociate them, which this does not prevent and no driver measured does.
fn rounded(x: f32) -> f32 {
    return bitcast<f32>(bitcast<u32>(x) ^ params.zero);
}

// The stored byte of a unorm channel.
fn byte(v: f32) -> f32 {
    return round(v * 255.0);
}

fn luma_at(p: vec2<i32>) -> f32 {
    let c = textureLoad(level0, p, 0);
    return rounded(params.luma.x * byte(c.r))
        + rounded(params.luma.y * byte(c.g))
        + rounded(params.luma.z * byte(c.b));
}

// How far up the ramp coordinate `i` of `len` is: 1, 2, ... `ramp`.
fn edge_step(i: i32, len: i32) -> i32 {
    return min(min(i, len - 1 - i) + 1 - i32(params.border), i32(params.ramp));
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(corners[i], 0.0, 1.0);
}

// Pass 1: the horizontal blur of the luma, over the columns `finish` changes
// and the rows it reads; zero elsewhere, never read.
@fragment
fn blur(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy);
    let b = i32(params.border);
    if p.x < b || p.x >= i32(params.width) - b || p.y >= i32(params.height) {
        return vec4<f32>(0.0);
    }
    var sum = rounded(params.kernel[0].x * luma_at(vec2<i32>(p.x - 3, p.y)));
    for (var k = 1; k < 7; k++) {
        sum = sum + rounded(params.kernel[k].x * luma_at(vec2<i32>(p.x - 3 + k, p.y)));
    }
    return vec4<f32>(sum, 0.0, 0.0, 0.0);
}

// Pass 2: the vertical blur, and each changed texel's R, G and B moved by
// `factor * (luma - blur)`, rounded half to even and clamped. The border,
// the padding past the image and alpha are level 0's.
@fragment
fn finish(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy);
    let c = textureLoad(level0, p, 0);
    let w = i32(params.width);
    let h = i32(params.height);
    let b = i32(params.border);
    if p.x < b || p.y < b || p.x >= w - b || p.y >= h - b {
        return c;
    }
    var blur = rounded(params.kernel[0].x * textureLoad(blurred, vec2<i32>(p.x, p.y - 3), 0).r);
    for (var k = 1; k < 7; k++) {
        blur = blur + rounded(params.kernel[k].x * textureLoad(blurred, vec2<i32>(p.x, p.y - 3 + k), 0).r);
    }
    let step = min(edge_step(p.y, h), edge_step(p.x, w));
    let delta = round(params.factors[step - 1] * (luma_at(p) - blur));
    let r = clamp(byte(c.r) + delta, 0.0, 255.0);
    let g = clamp(byte(c.g) + delta, 0.0, 255.0);
    let bl = clamp(byte(c.b) + delta, 0.0, 255.0);
    return vec4<f32>(r / 255.0, g / 255.0, bl / 255.0, c.a);
}
