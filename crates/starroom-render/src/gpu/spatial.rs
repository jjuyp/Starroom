//! GPU backend for the existing detail crate; CPU/export remain the numeric oracle.
use super::*;
use starroom_detail::{
    DenoiseParameters, LocalDetailParameters, SharpenParameters, gaussian_kernel,
};

#[derive(Clone, Copy)]
pub struct GpuDetailRequest {
    pub width: u32,
    pub height: u32,
    pub denoise: DenoiseParameters,
    pub local_detail: LocalDetailParameters,
    pub sharpen: SharpenParameters,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MergeParameters {
    shape: [u32; 4],
    local: [f32; 4],
    sharpen: [f32; 4],
    extra: [f32; 4],
}

impl GpuDetailRequest {
    pub(super) fn nr(self) -> GpuDenoiseRequest {
        GpuDenoiseRequest {
            width: self.width,
            height: self.height,
            parameters: self.denoise,
        }
    }

    pub(super) fn validate(self, count: usize) -> Result<(), GpuError> {
        self.nr().is_identity(count)?;
        let l = self.local_detail;
        let s = self.sharpen;
        if ![
            l.texture,
            l.clarity,
            l.dehaze,
            s.amount,
            s.radius,
            s.detail,
            s.masking,
            s.halo_protection,
            s.threshold,
        ]
        .into_iter()
        .all(f32::is_finite)
        {
            return Err(GpuError::InvalidPixels);
        }
        Ok(())
    }

    pub(super) fn active(self, kind: usize) -> bool {
        if kind == 0 {
            [
                self.local_detail.texture,
                self.local_detail.clarity,
                self.local_detail.dehaze,
            ]
            .into_iter()
            .any(|v| v.clamp(-1.0, 1.0).abs() > f32::EPSILON)
        } else {
            self.sharpen.amount.clamp(0.0, 2.0) > f32::EPSILON
        }
    }

    fn parameters(self, kind: usize) -> MergeParameters {
        let l = self.local_detail;
        let s = self.sharpen;
        MergeParameters {
            shape: [
                self.width,
                self.height,
                self.width * self.height,
                kind as u32,
            ],
            local: [
                l.texture.clamp(-1.0, 1.0),
                l.clarity.clamp(-1.0, 1.0),
                l.dehaze.clamp(-1.0, 1.0),
                0.0,
            ],
            sharpen: [
                s.amount.clamp(0.0, 2.0),
                s.detail.clamp(0.0, 1.0),
                s.masking.clamp(0.0, 1.0),
                s.halo_protection.clamp(0.0, 1.0),
            ],
            extra: [s.threshold.max(0.0), 0.0, 0.0, 0.0],
        }
    }
}

pub(super) struct GpuSpatialStage {
    filter: wgpu::ComputePipeline,
    merge: wgpu::ComputePipeline,
    filter_layout: wgpu::BindGroupLayout,
    merge_layout: wgpu::BindGroupLayout,
    buffers: [Option<Buffers>; 2],
}

struct Buffers {
    output: wgpu::Buffer,
    staging: wgpu::Buffer,
    horizontal: wgpu::Buffer,
    filtered: wgpu::Buffer,
    filter_parameters: wgpu::Buffer,
    merge_parameters: wgpu::Buffer,
    weights: wgpu::Buffer,
    timings: Vec<Option<GpuTimingResources>>,
    capacity: u64,
    blur_identity: Option<u64>,
    output_identity: Option<u64>,
}

pub(super) struct EncodedSpatial {
    pub output: wgpu::Buffer,
    pub staging: wgpu::Buffer,
    pub identity: u64,
    pub cache_hit: bool,
    pub executed: Vec<usize>,
    pub kind: usize,
}

impl GpuSpatialStage {
    pub(super) fn invalidate(&mut self) {
        for buffers in self.buffers.iter_mut().flatten() {
            buffers.blur_identity = None;
            buffers.output_identity = None;
        }
    }

    fn new(renderer: &GpuRenderer) -> Result<Self, GpuError> {
        let scope = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let uniform = |binding, dynamic, size| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: std::num::NonZeroU64::new(size),
            },
            count: None,
        };
        let filter_layout =
            renderer
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("starroom-spatial-filter-layout"),
                    entries: &[
                        storage_layout_entry(0, true),
                        storage_layout_entry(1, false),
                        storage_layout_entry(2, true),
                        uniform(3, true, 32),
                    ],
                });
        let merge_layout =
            renderer
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("starroom-spatial-merge-layout"),
                    entries: &[
                        storage_layout_entry(0, false),
                        storage_layout_entry(1, true),
                        uniform(2, false, 64),
                    ],
                });
        let pipeline = |layout: &wgpu::BindGroupLayout,
                        source: &'static str,
                        entry: &'static str| {
            let shader = renderer
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(entry),
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
                });
            let layout = renderer
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(entry),
                    bind_group_layouts: &[Some(layout)],
                    immediate_size: 0,
                });
            renderer
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&layout),
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
        };
        let filter = pipeline(
            &filter_layout,
            include_str!("spatial_filter.wgsl"),
            "filter_main",
        );
        let merge = pipeline(
            &merge_layout,
            include_str!("spatial_merge.wgsl"),
            "merge_main",
        );
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(GpuError::Shader(error.to_string()));
        }
        renderer.pipeline_count.fetch_add(2, Ordering::Relaxed);
        renderer.statistics().pipeline_create_count =
            renderer.pipeline_count.load(Ordering::Relaxed);
        Ok(Self {
            filter,
            merge,
            filter_layout,
            merge_layout,
            buffers: [None, None],
        })
    }

    pub(super) fn allocated_buffer_bytes(&self) -> u64 {
        self.buffers
            .iter()
            .flatten()
            .map(|b| {
                b.output.size()
                    + b.staging.size()
                    + b.horizontal.size()
                    + b.filtered.size()
                    + b.filter_parameters.size()
                    + b.merge_parameters.size()
                    + b.weights.size()
                    + b.timings
                        .iter()
                        .flatten()
                        .map(|t| t.resolve.size())
                        .sum::<u64>()
            })
            .sum()
    }

    fn encode(
        &mut self,
        renderer: &GpuRenderer,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Buffer,
        input_identity: u64,
        request: GpuDetailRequest,
        kind: usize,
    ) -> Result<EncodedSpatial, GpuError> {
        let count = request.width * request.height;
        let bytes = u64::from(count) * 16;
        if bytes * 2 > renderer.device.limits().max_storage_buffer_binding_size {
            return Err(GpuError::Unsupported(
                "spatial buffers exceed adapter storage limit".into(),
            ));
        }
        let radii = if kind == 0 {
            [0.8, 3.0]
        } else {
            [
                request.sharpen.radius.clamp(0.3, 4.0),
                (request.sharpen.radius * 2.2).clamp(0.8, 8.0),
            ]
        };
        let mut hash = DefaultHasher::new();
        input_identity.hash(&mut hash);
        request.width.hash(&mut hash);
        request.height.hash(&mut hash);
        for radius in radii {
            radius.to_bits().hash(&mut hash);
        }
        let blur_identity = hash.finish();
        let parameters = request.parameters(kind);
        // Only parameters that affect this stage participate in output identity.
        if kind == 0 {
            hash.write(bytemuck::cast_slice(&parameters.local));
        } else {
            hash.write(bytemuck::cast_slice(&parameters.sharpen));
            hash.write(bytemuck::cast_slice(&parameters.extra));
        }
        let identity = hash.finish();
        if self.buffers[kind]
            .as_ref()
            .is_none_or(|b| b.capacity < bytes)
        {
            let buffer = |label, size, usage| {
                renderer.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            self.buffers[kind] = Some(Buffers {
                output: buffer(
                    "spatial-output",
                    bytes,
                    wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                ),
                staging: buffer(
                    "spatial-final-readback",
                    bytes + 256,
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                ),
                horizontal: buffer("spatial-horizontal", bytes, wgpu::BufferUsages::STORAGE),
                filtered: buffer("spatial-filtered", bytes * 2, wgpu::BufferUsages::STORAGE),
                filter_parameters: buffer(
                    "spatial-filter-uniforms",
                    6 * 256,
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                ),
                merge_parameters: buffer(
                    "spatial-merge-uniforms",
                    64,
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                ),
                weights: buffer(
                    "spatial-coefficients",
                    98 * 4,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                ),
                timings: (0..7).map(|_| renderer.timing_resources()).collect(),
                capacity: bytes,
                blur_identity: None,
                output_identity: None,
            });
        }
        let b = self.buffers[kind]
            .as_mut()
            .ok_or_else(|| GpuError::Validation("spatial buffers unavailable".into()))?;
        let hit = b.output_identity == Some(identity);
        let mut executed = Vec::new();
        if !hit {
            if b.blur_identity != Some(blur_identity) {
                let kernels = radii.map(gaussian_kernel);
                let mut weights = [0.0f32; 98];
                for (i, kernel) in kernels.iter().enumerate() {
                    weights[i * 49..i * 49 + kernel.len()].copy_from_slice(kernel);
                }
                renderer
                    .queue
                    .write_buffer(&b.weights, 0, bytemuck::cast_slice(&weights));
                let passes = if kind == 0 { 6 } else { 4 };
                for index in 0..passes {
                    let axis = index % 2;
                    let box_filter = index >= 4;
                    let level = index / 2;
                    let reach = if box_filter {
                        12
                    } else {
                        (kernels[level].len() / 2) as u32
                    };
                    let filter_kind = if box_filter { 1 + axis as u32 } else { 0 };
                    let filter_params = [
                        request.width,
                        request.height,
                        count,
                        reach,
                        axis as u32,
                        if box_filter { 0 } else { level as u32 * 49 },
                        if axis == 1 && !box_filter {
                            level as u32 * count
                        } else {
                            0
                        },
                        filter_kind,
                    ];
                    renderer.queue.write_buffer(
                        &b.filter_parameters,
                        index as u64 * 256,
                        bytemuck::cast_slice(&filter_params),
                    );
                    let group = renderer
                        .device
                        .create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("spatial-filter-group"),
                            layout: &self.filter_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: if axis == 0 {
                                        input.as_entire_binding()
                                    } else {
                                        b.horizontal.as_entire_binding()
                                    },
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: if axis == 0 {
                                        b.horizontal.as_entire_binding()
                                    } else {
                                        b.filtered.as_entire_binding()
                                    },
                                },
                                wgpu::BindGroupEntry {
                                    binding: 2,
                                    resource: b.weights.as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 3,
                                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                        buffer: &b.filter_parameters,
                                        offset: 0,
                                        size: std::num::NonZeroU64::new(32),
                                    }),
                                },
                            ],
                        });
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("spatial-filter-pass"),
                        timestamp_writes: b.timings[index].as_ref().map(GpuTimingResources::writes),
                    });
                    pass.set_pipeline(&self.filter);
                    pass.set_bind_group(0, &group, &[(index * 256) as u32]);
                    pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
                    if b.timings[index].is_some() {
                        executed.push(index);
                    }
                }
                b.blur_identity = Some(blur_identity);
            }
            encoder.copy_buffer_to_buffer(input, 0, &b.output, 0, bytes);
            renderer
                .queue
                .write_buffer(&b.merge_parameters, 0, bytemuck::bytes_of(&parameters));
            let group = renderer
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("spatial-merge-group"),
                    layout: &self.merge_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: b.output.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: b.filtered.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: b.merge_parameters.as_entire_binding(),
                        },
                    ],
                });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("spatial-merge-pass"),
                timestamp_writes: b.timings[6].as_ref().map(GpuTimingResources::writes),
            });
            pass.set_pipeline(&self.merge);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
            if b.timings[6].is_some() {
                executed.push(6);
            }
            b.output_identity = Some(identity);
        }
        Ok(EncodedSpatial {
            output: b.output.clone(),
            staging: b.staging.clone(),
            identity,
            cache_hit: hit,
            executed,
            kind,
        })
    }

    pub(super) fn resolve_timing(
        &self,
        stage: &EncodedSpatial,
        encoder: &mut wgpu::CommandEncoder,
        staging: &wgpu::Buffer,
        offset: u64,
    ) {
        if let Some(b) = &self.buffers[stage.kind] {
            for (slot, index) in stage.executed.iter().enumerate() {
                if let Some(timing) = &b.timings[*index] {
                    timing.resolve_into(encoder, staging, offset + slot as u64 * 16);
                }
            }
        }
    }
}

impl GpuRenderer {
    pub(super) fn encode_spatial(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Buffer,
        identity: u64,
        request: GpuDetailRequest,
        kind: usize,
    ) -> Result<EncodedSpatial, GpuError> {
        let mut stage = self.spatial.lock().map_err(|_| GpuError::DeviceLost)?;
        if stage.is_none() {
            *stage = Some(GpuSpatialStage::new(self)?);
        }
        stage
            .as_mut()
            .ok_or_else(|| GpuError::Validation("spatial stage unavailable".into()))?
            .encode(self, encoder, input, identity, request, kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starroom_detail::{LinearImage, denoise, local_detail, sharpen};

    #[test]
    fn gpu_spatial_matches_cpu_hdr_borders_order_alpha_and_cache() {
        let Ok(renderer) = GpuRenderer::try_new() else {
            eprintln!("GPU unavailable; hardware release qualification remains required");
            return;
        };
        let mut cases = 0;
        for (width, height) in [(1, 1), (1, 9), (11, 1), (17, 13), (67, 31)] {
            let pixels: Vec<[f32; 4]> = (0..width * height)
                .map(|i| {
                    let v = [-0.15, 0.0, 0.001, 0.18, 0.7, 1.5, 4.0][i % 7];
                    [v, v * 0.7 + 0.01, v * 0.4 - 0.01, (i % 5) as f32 * 0.25]
                })
                .collect();
            let original = LinearImage::new(
                width,
                height,
                pixels.iter().flat_map(|p| p[..3].iter().copied()).collect(),
            )
            .unwrap();
            let mut creative = GpuCreativeParameters {
                values: [[0.0; 4]; 23],
            };
            creative.values[18] = [pixels.len() as f32, width as f32, height as f32, 0.0];
            for radius in [0.3, 1.0, 4.0] {
                for mode in 0..5 {
                    let request = GpuDetailRequest {
                        width: width as u32,
                        height: height as u32,
                        denoise: DenoiseParameters {
                            luminance: if mode == 4 { 0.5 } else { 0.0 },
                            ..Default::default()
                        },
                        local_detail: LocalDetailParameters {
                            texture: if mode == 0 { 0.0 } else { -0.7 },
                            clarity: if mode <= 1 { 0.0 } else { 0.8 },
                            dehaze: if mode <= 2 { 0.0 } else { -0.6 },
                        },
                        sharpen: SharpenParameters {
                            amount: if mode == 1 { 0.0 } else { 1.4 },
                            radius,
                            detail: 0.7,
                            masking: 0.4,
                            halo_protection: 0.8,
                            threshold: 0.02,
                        },
                    };
                    let expected = sharpen(
                        &local_detail(&denoise(&original, request.denoise), request.local_detail),
                        request.sharpen,
                    );
                    let before = renderer.resource_stats();
                    let actual = renderer
                        .apply_creative_with_detail(
                            &pixels,
                            &creative,
                            &[0.0; 8192],
                            Some("spatial-oracle"),
                            request,
                        )
                        .unwrap();
                    assert_eq!(
                        renderer.resource_stats().final_readback_count,
                        before.final_readback_count + 1
                    );
                    for (i, pixel) in actual.iter().enumerate() {
                        assert_eq!(pixel[3].to_bits(), pixels[i][3].to_bits());
                        for (c, value) in pixel[..3].iter().enumerate() {
                            let reference = expected.data[i * 3 + c];
                            assert!(value.is_finite());
                            assert!(
                                (pixel[c] - reference).abs() <= 2.0e-5 * (1.0 + reference.abs()),
                                "{width}x{height} radius{radius} mode{mode} pixel{i}/{c}: {} vs {reference}",
                                pixel[c]
                            );
                        }
                    }
                    let (cached, profile) = crate::profiling::capture(|| {
                        renderer.apply_creative_with_detail(
                            &pixels,
                            &creative,
                            &[0.0; 8192],
                            Some("spatial-oracle"),
                            request,
                        )
                    });
                    assert_eq!(cached.unwrap(), actual);
                    assert_eq!(profile.gpu_hardware_pass_count, 0);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 75);
        assert_eq!(renderer.resource_stats().pipeline_create_count, 5);
    }

    #[test]
    fn gpu_spatial_slider_reuses_filters_and_invalidates_upstream_and_radius() {
        let Ok(renderer) = GpuRenderer::try_new() else {
            return;
        };
        let pixels: Vec<_> = (0..221)
            .map(|i| [i as f32 / 221.0, 0.2, 0.6, 1.0])
            .collect();
        let mut creative = GpuCreativeParameters {
            values: [[0.0; 4]; 23],
        };
        creative.values[18] = [221.0, 17.0, 13.0, 0.0];
        let mut request = GpuDetailRequest {
            width: 17,
            height: 13,
            denoise: Default::default(),
            local_detail: LocalDetailParameters {
                texture: 0.5,
                ..Default::default()
            },
            sharpen: SharpenParameters {
                amount: 0.0,
                ..Default::default()
            },
        };
        let run = |r: GpuDetailRequest, source: &[[f32; 4]]| {
            crate::profiling::capture(|| {
                renderer.apply_creative_with_detail(
                    source,
                    &creative,
                    &[0.0; 8192],
                    Some("filter-cache"),
                    r,
                )
            })
        };
        let (first, cold) = run(request, &pixels);
        first.unwrap();
        request.local_detail.texture = 0.8;
        let (second, slider) = run(request, &pixels);
        second.unwrap();
        if cold.gpu_hardware_pass_count > 0 {
            assert_eq!(cold.gpu_hardware_pass_count, 8);
            assert_eq!(slider.gpu_hardware_pass_count, 1, "merge only");
        }
        let mut changed = pixels.clone();
        changed[0][0] += 0.1;
        let (changed_result, changed_profile) = run(request, &changed);
        changed_result.unwrap();
        if cold.gpu_hardware_pass_count > 0 {
            assert_eq!(changed_profile.gpu_hardware_pass_count, 8);
        }
        request.local_detail = Default::default();
        request.sharpen.amount = 0.8;
        let (sharp, sharp_cold) = run(request, &changed);
        sharp.unwrap();
        request.sharpen.amount = 1.0;
        let (sharp, sharp_slider) = run(request, &changed);
        sharp.unwrap();
        request.sharpen.radius = 3.0;
        let (sharp, radius) = run(request, &changed);
        sharp.unwrap();
        if cold.gpu_hardware_pass_count > 0 {
            assert_eq!(sharp_cold.gpu_hardware_pass_count, 5);
            assert_eq!(sharp_slider.gpu_hardware_pass_count, 1);
            assert_eq!(radius.gpu_hardware_pass_count, 5);
        }
        let mut invalid = creative;
        invalid.values[0][0] = f32::NAN;
        assert!(matches!(
            renderer.apply_creative_with_detail(
                &changed,
                &invalid,
                &[0.0; 8192],
                Some("filter-cache"),
                request
            ),
            Err(GpuError::InvalidPixels)
        ));
        let (recovered, recovery_profile) = run(request, &changed);
        let recovered = recovered.unwrap();
        if cold.gpu_hardware_pass_count > 0 {
            assert_eq!(recovery_profile.gpu_hardware_pass_count, 6);
        }
        assert_eq!(run(request, &changed).0.unwrap(), recovered);
        request.sharpen.radius = f32::NAN;
        assert!(matches!(
            run(request, &changed).0,
            Err(GpuError::InvalidPixels)
        ));
        assert_eq!(pixels[0], [0.0, 0.2, 0.6, 1.0]);
    }
}
