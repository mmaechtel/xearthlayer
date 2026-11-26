//! `texture.sharpen` in the GPU worker: level 0 sharpened between the mip
//! pass and the BC1 kernel, in the tile's one submission.
//!
//! The mask is [`Sharpen::apply`], the same operations in the same order, in
//! two render passes (`gpu_sharpen.wgsl`): `blur` writes the horizontal blur
//! of each row's luma into an `R32Float` target, `finish` blurs it vertically
//! and writes the sharpened level 0 into a texture of its own, which the BC1
//! kernel reads for level 0. The level 0 texture itself is left as uploaded,
//! so the mips are built from the unsharpened image, as on the CPU
//! (`MipmapStream::with_sharpen`). Its constants (kernel, luma weights,
//! border, ramp and the ramp's factors) come from `sharpen.rs` through the
//! uniform [`params_bytes`] writes.
//!
//! Every product the CPU adds is rounded to f32 before the add in the shader
//! (`rounded`), and the factors are computed on the CPU: on RADV (AMD Radeon
//! Graphics, RAPHAEL_MENDOCINO) a driver-fused FMA in the luma and in both
//! blurs, and the shader's own division by the ramp, left 29 texels of a
//! 4096 tile one step off. With both, the sharpened level 0 is byte for byte
//! the CPU's there. That is measured on that adapter only, not guaranteed
//! for other drivers.
//!
//! Made only with sharpening on ([`SharpenPipeline::new`]): with it off the
//! worker has no pipeline, no targets and no pass.

use crate::dds::gpu_mips::{buffer_with, LevelPlan, LevelTexture};
use crate::dds::sharpen::{Sharpen, BORDER, KERNEL, LUMA, RAMP};

/// The shader: `vs`, `blur` and `finish`.
const SHADER: &str = include_str!("gpu_sharpen.wgsl");

// The shader reads seven taps, three either side, so every changed texel's
// taps must lie inside the border; the factors fill one vec4.
const _: () = assert!(KERNEL.len() == 7 && BORDER >= 3 && RAMP >= 1 && RAMP <= 4);

/// The unit tests read the sharpened level 0 back; production never does.
const READ_BACK: wgpu::TextureUsages = if cfg!(test) {
    wgpu::TextureUsages::COPY_SRC
} else {
    wgpu::TextureUsages::empty()
};

/// Bytes of the shader's `Params`.
const PARAMS_BYTES: usize = 16 + 16 + 16 + 16 + 7 * 16;

/// The shader's uniform for an image `width` x `height` (level 0 without
/// its padding) sharpened by `sharpen`: the size, [`BORDER`], [`RAMP`], a
/// zero word (`rounded` in the shader), the ramp's factors
/// ([`Sharpen::ramp_factor`]), the luma weights and the kernel, little
/// endian as WGSL's uniform layout places them.
pub(crate) fn params_bytes(sharpen: Sharpen, width: u32, height: u32) -> Vec<u8> {
    let mut words: Vec<u32> = vec![width, height, BORDER as u32, RAMP as u32, 0, 0, 0, 0];
    let mut factors = [0.0f32; 4];
    for (step, factor) in factors.iter_mut().enumerate().take(RAMP) {
        *factor = sharpen.ramp_factor(step + 1);
    }
    words.extend(factors.map(f32::to_bits));
    words.extend([LUMA[0], LUMA[1], LUMA[2], 0.0].map(f32::to_bits));
    for weight in KERNEL {
        words.extend([weight.to_bits(), 0, 0, 0]);
    }
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    debug_assert_eq!(bytes.len(), PARAMS_BYTES);
    bytes
}

/// The sharpening's two render pipelines, made once per worker, and only
/// with sharpening on.
pub(crate) struct SharpenPipeline {
    sharpen: Sharpen,
    blur: wgpu::RenderPipeline,
    finish: wgpu::RenderPipeline,
}

impl SharpenPipeline {
    /// The pipelines for `sharpen`, `None` when it is off.
    pub(crate) fn new(device: &wgpu::Device, sharpen: Sharpen) -> Option<Self> {
        if sharpen.is_off() {
            return None;
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tile-sharpen"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = |entry: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Some(Self {
            sharpen,
            blur: pipeline("blur", wgpu::TextureFormat::R32Float),
            finish: pipeline("finish", wgpu::TextureFormat::Rgba8Unorm),
        })
    }
}

/// One resource set's sharpening targets, at level 0's padded size: the
/// blurred rows (`R32Float`) and the sharpened level 0 (`Rgba8Unorm`), 64
/// MiB each for a 4096 tile, with the two passes' bind groups. Made with the
/// set's chain and kept with it.
pub(crate) struct SharpenTargets {
    blurred: LevelTexture,
    sharp: LevelTexture,
    blur_bind: wgpu::BindGroup,
    finish_bind: wgpu::BindGroup,
}

impl SharpenTargets {
    /// The targets that sharpen `level0`, the chain's level 0 texture of
    /// `plan`, with `pipeline`.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &SharpenPipeline,
        level0: &LevelTexture,
        plan: &LevelPlan,
    ) -> Self {
        let target = |label, format, extra| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: plan.padded_width,
                    height: plan.padded_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | extra,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            LevelTexture { texture, view }
        };
        let blurred = target(
            "tile-sharpen-blurred",
            wgpu::TextureFormat::R32Float,
            wgpu::TextureUsages::empty(),
        );
        let sharp = target(
            "tile-level0-sharp",
            wgpu::TextureFormat::Rgba8Unorm,
            READ_BACK,
        );
        let params = buffer_with(
            device,
            queue,
            "tile-sharpen-params",
            wgpu::BufferUsages::UNIFORM,
            &params_bytes(pipeline.sharpen, plan.width, plan.height),
        );
        let bind = |render: &wgpu::RenderPipeline, with_blurred: bool| {
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&level0.view),
                },
            ];
            if with_blurred {
                entries.push(wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&blurred.view),
                });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tile-sharpen"),
                layout: &render.get_bind_group_layout(0),
                entries: &entries,
            })
        };
        let blur_bind = bind(&pipeline.blur, false);
        let finish_bind = bind(&pipeline.finish, true);
        Self {
            blurred,
            sharp,
            blur_bind,
            finish_bind,
        }
    }

    /// The sharpened level 0, which the BC1 kernel reads in place of the
    /// chain's level 0.
    pub(crate) fn level0(&self) -> &LevelTexture {
        &self.sharp
    }

    /// Records the two passes into `encoder`, after the mip pass has read
    /// level 0 and before the BC1 pass. `timestamps` are the begin of the
    /// first pass and the end of the second (`KernelTimer::span_writes`).
    pub(crate) fn record(
        &self,
        pipeline: &SharpenPipeline,
        encoder: &mut wgpu::CommandEncoder,
        timestamps: Option<(
            wgpu::RenderPassTimestampWrites<'_>,
            wgpu::RenderPassTimestampWrites<'_>,
        )>,
    ) {
        let (begin, end) = timestamps.unzip();
        for (render, bind, target, timestamp_writes) in [
            (&pipeline.blur, &self.blur_bind, &self.blurred.view, begin),
            (&pipeline.finish, &self.finish_bind, &self.sharp.view, end),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tile-sharpen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Every texel is written: nothing to load.
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(render);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dds::compressor::pad_to_block_multiple;
    use crate::dds::compressor::tests::varied;
    use crate::dds::gpu_mips::{chain_plan, upload_row_bytes};
    use crate::dds::DdsFormat;
    use image::RgbaImage;

    fn words(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()
    }

    /// The uniform holds `sharpen.rs`'s own constants, bit for bit, at the
    /// offsets of the shader's `Params`, and the factors `apply` uses.
    #[test]
    fn the_params_are_sharpen_rs_constants_at_the_shaders_offsets() {
        for tenths in 1u8..=10 {
            let sharpen = Sharpen::from_strength(f64::from(tenths) / 10.0);
            let w = words(&params_bytes(sharpen, 4093, 17));
            assert_eq!(w.len() * 4, PARAMS_BYTES);
            assert_eq!(&w[..5], &[4093, 17, BORDER as u32, RAMP as u32, 0]);
            for step in 1..=RAMP {
                // As `apply` computes it: strength × step / RAMP in f32.
                let strength = f32::from(tenths) / 10.0;
                let want = strength * step as f32 / RAMP as f32;
                assert_eq!(w[8 + step - 1], want.to_bits(), "factor {step}");
                assert_eq!(w[8 + step - 1], sharpen.ramp_factor(step).to_bits());
            }
            for (i, weight) in LUMA.iter().enumerate() {
                assert_eq!(w[12 + i], weight.to_bits(), "luma {i}");
            }
            for (k, weight) in KERNEL.iter().enumerate() {
                assert_eq!(w[16 + 4 * k], weight.to_bits(), "kernel {k}");
            }
        }
    }

    /// The shader compiles, with no GPU: naga parses and validates it and
    /// writes each entry point as Metal (MSL, at the versions macOS 15 and
    /// 26 give wgpu) and as Vulkan's SPIR-V.
    #[test]
    fn the_shader_compiles_for_metal_and_vulkan_with_its_three_entry_points() {
        use naga::back::{msl, spv};
        use naga::valid::{Capabilities, ValidationFlags, Validator};

        let module = naga::front::wgsl::parse_str(SHADER)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(SHADER)));
        let info = Validator::new(ValidationFlags::all(), Capabilities::default())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(SHADER)));
        for (entry, stage) in [
            ("vs", naga::ShaderStage::Vertex),
            ("blur", naga::ShaderStage::Fragment),
            ("finish", naga::ShaderStage::Fragment),
        ] {
            assert!(
                module
                    .entry_points
                    .iter()
                    .any(|ep| ep.name == entry && ep.stage == stage),
                "{entry} is a {stage:?} entry point"
            );
            for lang_version in [(3, 2), (4, 0)] {
                let options = msl::Options {
                    lang_version,
                    ..msl::Options::default()
                };
                let pipeline = msl::PipelineOptions {
                    entry_point: Some((stage, entry.to_string())),
                    ..msl::PipelineOptions::default()
                };
                let (source, _) = msl::write_string(&module, &info, &options, &pipeline)
                    .unwrap_or_else(|e| panic!("{entry} in MSL {lang_version:?}: {e}"));
                assert!(source.contains(entry), "{entry} in MSL {lang_version:?}");
            }
            let pipeline = spv::PipelineOptions {
                shader_stage: stage,
                entry_point: entry.to_string(),
            };
            let words = spv::write_vec(&module, &info, &spv::Options::default(), Some(&pipeline))
                .unwrap_or_else(|e| panic!("{entry} in SPIR-V: {e}"));
            assert_eq!(words.first(), Some(&0x0723_0203), "{entry} in SPIR-V");
        }
    }

    // =========================================================================
    // The shader on hardware (ignored)
    // =========================================================================

    /// A device on `XEL_TEST_GPU_DEVICE` (`integrated` by default), or
    /// `None` when the host has no such GPU.
    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let selector =
            std::env::var("XEL_TEST_GPU_DEVICE").unwrap_or_else(|_| "integrated".to_string());
        let health = crate::dds::gpu_health::SharedGpuHealth::new();
        crate::dds::create_gpu_resources(&selector, &health)
            .ok()
            .map(|(device, queue, _, _)| (device, queue))
    }

    /// `image` sharpened by the shader: uploaded padded as the worker
    /// uploads it, both passes run, the sharpened level 0 read back at its
    /// padded size.
    fn gpu_sharpened(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &SharpenPipeline,
        image: &RgbaImage,
    ) -> RgbaImage {
        let plan = chain_plan(image.width(), image.height(), 1, DdsFormat::BC1)[0];
        let padded = pad_to_block_multiple(image);
        let size = wgpu::Extent3d {
            width: plan.padded_width,
            height: plan.padded_height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            padded.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(plan.padded_width * 4),
                rows_per_image: Some(plan.padded_height),
            },
            size,
        );
        let view = texture.create_view(&Default::default());
        let level0 = LevelTexture { texture, view };
        let targets = SharpenTargets::new(device, queue, pipeline, &level0, &plan);
        let row = upload_row_bytes(plan.padded_width);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * plan.padded_height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        targets.record(pipeline, &mut encoder, None);
        encoder.copy_texture_to_buffer(
            targets.level0().texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(plan.padded_height),
                },
            },
            size,
        );
        queue.submit(std::iter::once(encoder.finish()));
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the GPU finishes");
        let data = buffer.slice(..).get_mapped_range().unwrap();
        let texels = (plan.padded_width * 4) as usize;
        let bytes = (0..plan.padded_height as usize)
            .flat_map(|y| data[y * row as usize..][..texels].to_vec())
            .collect();
        RgbaImage::from_raw(plan.padded_width, plan.padded_height, bytes).unwrap()
    }

    /// Scene plus noise, with noisy alpha in its lower part: the image of
    /// the measurement (`issues/2026-11-26_sharpen-gpu`).
    fn test_image(w: u32, h: u32) -> RgbaImage {
        let mut image = varied(w, h);
        for (i, p) in image.pixels_mut().enumerate() {
            let z = (i as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .rotate_left(17);
            for c in 0..3 {
                p[c] = p[c].saturating_add((z >> (8 * c)) as u8 % 24);
            }
            if i / w as usize > h as usize * 3 / 4 {
                p[3] = (z >> 40) as u8;
            }
        }
        image
    }

    /// The shader's level 0 is `Sharpen::apply`'s, padding included, byte
    /// for byte, on the 4096 tile, odd and non-square sizes, a size padded
    /// on both axes, one just big enough to change a texel and ones too
    /// small to change any, at several strengths. Measured on the adapter
    /// the tests run on; not a guarantee for other drivers.
    #[test]
    #[ignore] // Requires GPU hardware
    fn the_gpu_sharpens_level_0_as_the_cpu_does() {
        let Some((device, queue)) = device() else {
            eprintln!("Skipping: no usable GPU");
            return;
        };
        assert!(SharpenPipeline::new(&device, Sharpen::OFF).is_none());
        for tenths in 1u8..=10 {
            let sharpen = Sharpen::from_strength(f64::from(tenths) / 10.0);
            let pipeline = SharpenPipeline::new(&device, sharpen).expect("on");
            for (w, h) in [
                (4096, 4096),
                (1001, 777),
                (37, 23),
                (203, 131),
                (9, 9),
                (8, 8),
                (6, 6),
                (64, 1),
            ] {
                if tenths != 2 && w == 4096 {
                    continue;
                }
                let image = test_image(w, h);
                let mut cpu = image.clone();
                sharpen.apply(&mut cpu);
                let want = pad_to_block_multiple(&cpu).into_owned();
                let got = gpu_sharpened(&device, &queue, &pipeline, &image);
                assert_eq!(got.dimensions(), want.dimensions(), "{w}x{h}");
                let differ: Vec<usize> = got
                    .as_raw()
                    .iter()
                    .zip(want.as_raw())
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(i, _)| i)
                    .collect();
                assert!(
                    differ.is_empty(),
                    "strength {sharpen}, {w}x{h}: {} bytes differ, first at texel {:?}",
                    differ.len(),
                    differ
                        .first()
                        .map(|i| (i / 4 % want.width() as usize, i / 4 / want.width() as usize))
                );
            }
        }
    }

    /// Release-build GPU time of both passes on a 4096 tile at 0.2. Run
    /// with `cargo test --release -p xearthlayer --lib
    /// time_the_gpu_sharpen_passes -- --ignored --nocapture`.
    #[test]
    #[ignore] // Requires GPU hardware
    fn time_the_gpu_sharpen_passes() {
        use crate::dds::gpu_timestamps::KernelTimer;
        let Some((device, queue)) = device() else {
            eprintln!("Skipping: no usable GPU");
            return;
        };
        let Some(timer) = KernelTimer::new(&device, &queue, 1) else {
            eprintln!("Skipping: no timestamp queries");
            return;
        };
        let pipeline = SharpenPipeline::new(&device, Sharpen::from_strength(0.2)).unwrap();
        let image = test_image(4096, 4096);
        let plan = chain_plan(4096, 4096, 1, DdsFormat::BC1)[0];
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 4096,
                height: 4096,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            image.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4096 * 4),
                rows_per_image: Some(4096),
            },
            texture.size(),
        );
        let view = texture.create_view(&Default::default());
        let level0 = LevelTexture { texture, view };
        let targets = SharpenTargets::new(&device, &queue, &pipeline, &level0, &plan);
        let mut runs: Vec<f64> = (0..23)
            .map(|_| {
                let mut encoder = device.create_command_encoder(&Default::default());
                targets.record(&pipeline, &mut encoder, Some(timer.span_writes(0)));
                timer.resolve(&mut encoder, 1);
                queue.submit(std::iter::once(encoder.finish()));
                let mapped = timer.map(1);
                device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .expect("the GPU finishes");
                timer.read(&mapped, 1).unwrap()[0].as_secs_f64() * 1e3
            })
            .skip(3)
            .collect();
        runs.sort_by(f64::total_cmp);
        println!(
            "GPU sharpen, both passes, 4096 at 0.2: median {:.2} ms, min {:.2} ms",
            runs[runs.len() / 2],
            runs[0]
        );
    }
}
