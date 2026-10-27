//! Ectomatter's frame on the GPU, in the screen's own color space.
//!
//! Ectomatter composes each frame on the CPU as the Wayland compositor is
//! given it (picture.rs): premultiplied RGBA, four 16-bit values a pixel,
//! encoded as PQ (SMPTE ST 2084, nits up to 10,000) with sRGB's primaries
//! (BT.709), the desktop's white at its `reference` nits. A shell that
//! presents through wgpu uploads that frame as it is, and `Presenter` draws it
//! into the surface's texture as the surface's color space wants it:
//!
//! - **HDR10** (`Rgb10a2Unorm`, `SurfaceColorSpace::Bt2100Pq`): PQ again, with
//!   BT.2020's primaries.
//! - **scRGB** (`Rgba16Float`, `SurfaceColorSpace::ExtendedSrgbLinear`): linear
//!   light with sRGB's primaries, 1.0 being 80 nits, so the desktop's white
//!   is `reference / 80` and lit keys go above it.
//! - **SDR** (an sRGB format): the desktop's white at 1.0, brighter clipped to
//!   it, as a screen without HDR shows the page.
//!
//! wgpu applies no encoding of its own to any of these (wgpu 30, `lib.rs`),
//! so the values written are the values shown.

use std::future::Future;

/// The wgpu this presents with, for the shell that makes its surface.
pub use wgpu;

/// What the surface shows: its color space, as the presenter writes for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Hdr10,
    ScRgb,
    Sdr,
}

impl Target {
    /// The texture format a surface is configured with for this target, and
    /// its color space.
    pub fn surface(self) -> (wgpu::TextureFormat, wgpu::SurfaceColorSpace) {
        match self {
            Target::Hdr10 => (wgpu::TextureFormat::Rgb10a2Unorm, wgpu::SurfaceColorSpace::Bt2100Pq),
            Target::ScRgb => (wgpu::TextureFormat::Rgba16Float, wgpu::SurfaceColorSpace::ExtendedSrgbLinear),
            Target::Sdr => (wgpu::TextureFormat::Bgra8UnormSrgb, wgpu::SurfaceColorSpace::Srgb),
        }
    }

    fn mode(self) -> u32 {
        match self {
            Target::Hdr10 => 0,
            Target::ScRgb => 1,
            Target::Sdr => 2,
        }
    }
}

/// The best target a surface offers, HDR first: HDR10 where it takes PQ,
/// else scRGB, else SDR in whichever sRGB format it has.
pub fn best_target(caps: &wgpu::SurfaceCapabilities) -> (Target, wgpu::TextureFormat) {
    let offers = |f: wgpu::TextureFormat, cs: wgpu::SurfaceColorSpace| cs.to_color_spaces().is_some_and(|set| caps.color_spaces(f).contains(set));
    for t in [Target::Hdr10, Target::ScRgb] {
        let (f, cs) = t.surface();
        if offers(f, cs) {
            return (t, f);
        }
    }
    let srgb = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
    (Target::Sdr, srgb)
}

const SHADER: &str = r#"
struct Params {
    reference: f32,
    mode: u32,
    _a: u32,
    _b: u32,
};
@group(0) @binding(0) var frame: texture_2d<u32>;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // one triangle over the whole target
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

const M1: f32 = 0.1593017578125;
const M2: f32 = 78.84375;
const C1: f32 = 0.8359375;
const C2: f32 = 18.8515625;
const C3: f32 = 18.6875;

// PQ's code value (0 to 1) as nits, and nits as its code value
fn pq_to_nits(e: vec3<f32>) -> vec3<f32> {
    let p = pow(max(e, vec3<f32>(0.0)), vec3<f32>(1.0 / M2));
    return 10000.0 * pow(max(p - C1, vec3<f32>(0.0)) / (C2 - C3 * p), vec3<f32>(1.0 / M1));
}
fn nits_to_pq(n: vec3<f32>) -> vec3<f32> {
    let y = pow(max(n, vec3<f32>(0.0)) / 10000.0, vec3<f32>(M1));
    return pow((C1 + C2 * y) / (1.0 + C3 * y), vec3<f32>(M2));
}

@fragment
fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let px = textureLoad(frame, vec2<i32>(at.xy), 0);
    let v = vec4<f32>(px) / 65535.0;
    let a = v.a;
    if (a <= 0.0) {
        return vec4<f32>(0.0);
    }
    // straight, in nits, with sRGB's primaries
    let nits = pq_to_nits(v.rgb / a);
    var rgb: vec3<f32>;
    if (params.mode == 0u) {
        // BT.709 to BT.2020 (ITU-R BT.2087)
        let m = mat3x3<f32>(
            vec3<f32>(0.6274039, 0.0690973, 0.0163914),
            vec3<f32>(0.3292830, 0.9195404, 0.0880133),
            vec3<f32>(0.0433131, 0.0113623, 0.8955953));
        rgb = nits_to_pq(m * nits);
    } else if (params.mode == 1u) {
        rgb = nits / 80.0;
    } else {
        rgb = clamp(nits / params.reference, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(rgb * a, a);
}
"#;

/// Draws Ectomatter's frame into a target texture of one format, for one
/// target: the frame uploaded as it is, converted as it's drawn.
pub struct Presenter {
    target: Target,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    frame: Option<(wgpu::Texture, wgpu::BindGroup, (u32, u32))>,
}

impl Presenter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, target: Target) -> Presenter {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("ectomatter frame"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ectomatter frame"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ectomatter frame"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ectomatter frame"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor { label: Some("ectomatter frame"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        Presenter { target, pipeline, layout, params, frame: None }
    }

    pub fn target(&self) -> Target {
        self.target
    }

    /// The frame, `w` by `h`, as Ectomatter composed it (its rows `w` long,
    /// four 16-bit values a pixel); only `dirty`'s boxes (x, y, w, h) copied
    /// where the last frame was the same size.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, (w, h): (u32, u32), frame: &[[u16; 4]], dirty: &[(u32, u32, u32, u32)]) {
        assert_eq!(frame.len(), (w * h) as usize, "a frame {w} by {h}");
        let fresh = self.frame.as_ref().is_none_or(|(_, _, size)| *size != (w, h));
        if fresh {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ectomatter frame"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ectomatter frame"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: self.params.as_entire_binding() }],
            });
            self.frame = Some((texture, group, (w, h)));
        }
        let (texture, _, _) = self.frame.as_ref().unwrap();
        let all = [(0, 0, w, h)];
        let boxes: &[(u32, u32, u32, u32)] = if fresh || dirty.is_empty() { &all } else { dirty };
        let bytes: &[u8] = bytemuck::cast_slice(frame);
        for &(x, y, bw, bh) in boxes {
            let (x, y) = (x.min(w), y.min(h));
            let (bw, bh) = (bw.min(w - x), bh.min(h - y));
            if bw == 0 || bh == 0 {
                continue;
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
                &bytes[((y * w + x) * 8) as usize..],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 8), rows_per_image: Some(bh) },
                wgpu::Extent3d { width: bw, height: bh, depth_or_array_layers: 1 },
            );
        }
    }

    /// Draw the frame last uploaded into `view`, for a desktop whose white is
    /// `reference` nits.
    pub fn draw(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, reference: f32) {
        let Some((_, group, _)) = &self.frame else { return };
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&[reference.to_bits(), self.target.mode(), 0, 0]));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ectomatter frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// A future run to its end on this thread: wgpu's own futures are ready once
/// the device is polled, which is all a native shell needs.
pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device on Mesa's lavapipe, Vulkan on the CPU: no GPU is touched.
    fn cpu_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let icd = "/usr/share/vulkan/icd.d/lvp_icd.x86_64.json";
        assert!(std::path::Path::new(icd).exists(), "these tests need Mesa's lavapipe ({icd})");
        // (set before any Vulkan loader runs in this test binary; every test
        // here asks for the same)
        unsafe { std::env::set_var("VK_ICD_FILENAMES", icd) };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { force_fallback_adapter: true, ..Default::default() })).expect("lavapipe's adapter");
        assert_eq!(adapter.get_info().device_type, wgpu::DeviceType::Cpu, "lavapipe, not a GPU");
        Some(block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("a device on lavapipe"))
    }

    fn pq(nits: f64) -> f64 {
        let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
        let (c1, c2, c3) = (3424.0 / 4096.0, 2413.0 / 4096.0 * 32.0, 2392.0 / 4096.0 * 32.0);
        let y = (nits.max(0.0) / 10000.0).powf(m1);
        ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
    }

    /// A frame of greys and colors at known nits, some see-through.
    fn frame() -> (u32, u32, Vec<[u16; 4]>, Vec<([f64; 3], f64)>) {
        let px: Vec<([f64; 3], f64)> = vec![
            ([0.0, 0.0, 0.0], 1.0),
            ([203.0, 203.0, 203.0], 1.0),
            ([1000.0, 500.0, 100.0], 1.0),
            ([50.0, 400.0, 2000.0], 1.0),
            ([203.0, 0.0, 0.0], 0.5),
            ([0.0, 0.0, 0.0], 0.0),
            ([10.0, 10.0, 10.0], 1.0),
            ([4000.0, 4000.0, 4000.0], 1.0),
        ];
        let enc = |n: f64, a: f64| ((pq(n) * a) * 65535.0).round() as u16;
        let data = px.iter().map(|(c, a)| [enc(c[0], *a), enc(c[1], *a), enc(c[2], *a), (a * 65535.0).round() as u16]).collect();
        (px.len() as u32, 1, data, px)
    }

    /// The frame drawn for `target` into a texture of `format`, read back as
    /// each pixel's four values.
    fn drawn(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, target: Target, reference: f32) -> Vec<[f64; 4]> {
        let (w, h, data, _) = frame();
        let mut p = Presenter::new(device, format, target);
        p.upload(device, queue, (w, h), &data, &[]);
        let out = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = out.create_view(&Default::default());
        let bpp = format.block_copy_size(None).unwrap();
        let row = (w * bpp).div_ceil(256) * 256;
        let back = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&Default::default());
        p.draw(queue, &mut enc, &view, reference);
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &out, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &back, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);
        back.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = back.slice(..).get_mapped_range().expect("the read-back mapped").to_vec();
        (0..w as usize)
            .map(|x| {
                let b = &bytes[x * bpp as usize..];
                match format {
                    wgpu::TextureFormat::Rgba16Float => {
                        let h = |i: usize| half(u16::from_le_bytes([b[i * 2], b[i * 2 + 1]]));
                        [h(0), h(1), h(2), h(3)]
                    }
                    wgpu::TextureFormat::Rgb10a2Unorm => {
                        let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                        [(v & 1023) as f64 / 1023.0, ((v >> 10) & 1023) as f64 / 1023.0, ((v >> 20) & 1023) as f64 / 1023.0, (v >> 30) as f64 / 3.0]
                    }
                    _ => [b[0] as f64 / 255.0, b[1] as f64 / 255.0, b[2] as f64 / 255.0, b[3] as f64 / 255.0],
                }
            })
            .collect()
    }

    fn half(h: u16) -> f64 {
        let (s, e, m) = ((h >> 15) as i32, ((h >> 10) & 31) as i32, (h & 1023) as f64);
        let v = if e == 0 { m / 1024.0 * 2f64.powi(-14) } else { (1.0 + m / 1024.0) * 2f64.powi(e - 15) };
        if s == 1 { -v } else { v }
    }

    #[test]
    fn scrgb_is_linear_light_at_80_nits_to_1() {
        let Some((device, queue)) = cpu_device() else { return };
        let (_, _, _, px) = frame();
        let got = drawn(&device, &queue, wgpu::TextureFormat::Rgba16Float, Target::ScRgb, 203.0);
        for (g, (c, a)) in got.iter().zip(&px) {
            for i in 0..3 {
                let want = c[i] / 80.0 * a;
                assert!((g[i] - want).abs() <= want.abs() * 0.01 + 0.002, "{g:?} for {c:?} at {a}");
            }
            assert!((g[3] - a).abs() < 0.002);
        }
    }

    #[test]
    fn hdr10_is_pq_with_bt2020s_primaries() {
        let Some((device, queue)) = cpu_device() else { return };
        let (_, _, _, px) = frame();
        let got = drawn(&device, &queue, wgpu::TextureFormat::Rgb10a2Unorm, Target::Hdr10, 203.0);
        let m = [[0.6274039, 0.3292830, 0.0433131], [0.0690973, 0.9195404, 0.0113623], [0.0163914, 0.0880133, 0.8955953]];
        for (g, (c, _)) in got.iter().zip(&px).filter(|(_, (_, a))| *a == 1.0) {
            for i in 0..3 {
                let n = m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2];
                assert!((g[i] - pq(n)).abs() <= 1.5 / 1023.0, "{g:?} for {c:?}: channel {i} wants {}", pq(n));
            }
        }
    }

    #[test]
    fn sdr_puts_the_desktops_white_at_1_and_clips_above() {
        let Some((device, queue)) = cpu_device() else { return };
        let (_, _, _, px) = frame();
        // (an sRGB format: read back as its encoding)
        let got = drawn(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb, Target::Sdr, 203.0);
        let srgb = |v: f64| if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        for (g, (c, _)) in got.iter().zip(&px).filter(|(_, (_, a))| *a == 1.0) {
            for i in 0..3 {
                let want = srgb((c[i] / 203.0).clamp(0.0, 1.0));
                assert!((g[i] - want).abs() <= 1.5 / 255.0, "{g:?} for {c:?}");
            }
        }
        assert!(got[7][..3].iter().all(|v| *v > 0.99), "4000 nits clipped to white");
    }

    #[test]
    fn an_upload_of_a_box_changes_only_the_box() {
        let Some((device, queue)) = cpu_device() else { return };
        let (w, h, mut data, _) = frame();
        let mut p = Presenter::new(&device, wgpu::TextureFormat::Rgba16Float, Target::ScRgb);
        p.upload(&device, &queue, (w, h), &data, &[]);
        data[2] = [0, 0, 0, 65535];
        p.upload(&device, &queue, (w, h), &data, &[(2, 0, 1, 1)]);
        // (drawn through the same steps as `drawn`, with this presenter)
        let out = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = out.create_view(&Default::default());
        let back = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 256, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&Default::default());
        p.draw(&queue, &mut enc, &view, 203.0);
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &out, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &back, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) } },
            wgpu::Extent3d { width: w, height: 1, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);
        back.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let b = back.slice(..).get_mapped_range().expect("the read-back mapped").to_vec();
        let red = |x: usize| half(u16::from_le_bytes([b[x * 8], b[x * 8 + 1]]));
        assert!(red(2) < 0.001, "the box changed: {}", red(2));
        assert!((red(1) - 203.0 / 80.0).abs() < 0.03, "beside it kept: {}", red(1));
    }
}
