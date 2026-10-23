//! Same NR semantics as the CPU oracle, operating on the retained creative output.
use super::*;
use starroom_detail::DenoiseParameters;

#[derive(Clone, Copy)]
pub struct GpuDenoiseRequest {
    pub width: u32,
    pub height: u32,
    pub parameters: DenoiseParameters,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Parameters {
    shape: [u32; 4],
    strengths: [f32; 4],
}

impl GpuDenoiseRequest {
    pub fn is_identity(self, count: usize) -> Result<bool, GpuError> {
        let (parameters, _) = self.prepare(count)?;
        Ok(parameters.strengths[0] <= f32::EPSILON && parameters.strengths[1] <= f32::EPSILON)
    }
    fn prepare(self, count: usize) -> Result<(Parameters, Vec<f32>), GpuError> {
        let p = self.parameters;
        if self.width == 0
            || self.height == 0
            || u64::from(self.width) * u64::from(self.height) != count as u64
            || count > u32::MAX as usize
            || ![
                p.luminance,
                p.chroma,
                p.radius,
                p.detail_protection,
                p.high_iso,
            ]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(GpuError::InvalidPixels);
        }
        let iso = p.high_iso.clamp(0.0, 1.0);
        let radius = p.radius.clamp(0.6, 4.0);
        let half = (radius * 2.0).ceil() as i32;
        let protection = p.detail_protection.clamp(0.0, 1.0);
        let parameters = Parameters {
            shape: [self.width, self.height, count as u32, half as u32],
            strengths: [
                (p.luminance + iso * 0.35).clamp(0.0, 1.0),
                (p.chroma + iso * 0.55).clamp(0.0, 1.0),
                0.018 + (1.0 - protection) * 0.12 + iso * 0.08,
                protection,
            ],
        };
        let mut weights = Vec::new();
        for dy in -half..=half {
            for dx in -half..=half {
                weights.push((-0.5 * ((dx * dx + dy * dy) as f32) / (radius * radius)).exp());
            }
        }
        Ok((parameters, weights))
    }
}

pub(super) struct GpuDenoiseStage {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    buffers: Option<Buffers>,
}

struct Buffers {
    output: wgpu::Buffer,
    staging: wgpu::Buffer,
    parameters: wgpu::Buffer,
    weights: wgpu::Buffer,
    timing: Option<GpuTimingResources>,
    capacity_bytes: u64,
    identity: Option<u64>,
}

pub(super) struct EncodedDenoise {
    pub identity: u64,
    pub output: wgpu::Buffer,
    pub staging: wgpu::Buffer,
    pub timestamps: bool,
    pub cache_hit: bool,
}

impl GpuDenoiseStage {
    pub(super) fn invalidate(&mut self) {
        if let Some(buffers) = &mut self.buffers {
            buffers.identity = None;
        }
    }

    pub(super) fn new(renderer: &GpuRenderer) -> Result<Self, GpuError> {
        let scope = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let layout = renderer
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("starroom-denoise-layout"),
                entries: &[
                    storage_layout_entry(0, true),
                    storage_layout_entry(1, false),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    storage_layout_entry(3, true),
                ],
            });
        let pipeline_layout =
            renderer
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("starroom-denoise-pipeline-layout"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
        let shader = renderer
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("starroom-denoise-wgsl"),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("denoise.wgsl"))),
            });
        let pipeline = renderer
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("starroom-denoise-pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("denoise_main"),
                compilation_options: Default::default(),
                cache: None,
            });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(GpuError::Shader(error.to_string()));
        }
        renderer.pipeline_count.fetch_add(1, Ordering::Relaxed);
        renderer.statistics().pipeline_create_count =
            renderer.pipeline_count.load(Ordering::Relaxed);
        Ok(Self {
            pipeline,
            layout,
            buffers: None,
        })
    }

    pub(super) fn allocated_buffer_bytes(&self) -> u64 {
        self.buffers.as_ref().map_or(0, |b| {
            b.output.size()
                + b.staging.size()
                + b.parameters.size()
                + b.weights.size()
                + b.timing.as_ref().map_or(0, |t| t.resolve.size())
        })
    }

    fn encode(
        &mut self,
        renderer: &GpuRenderer,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Buffer,
        creative_identity: u64,
        request: GpuDenoiseRequest,
        byte_len: u64,
    ) -> Result<EncodedDenoise, GpuError> {
        let (parameters, weights) = request.prepare((byte_len / 16) as usize)?;
        let mut hash = DefaultHasher::new();
        creative_identity.hash(&mut hash);
        hash.write(bytemuck::bytes_of(&parameters));
        hash.write(bytemuck::cast_slice(&weights));
        let identity = hash.finish();
        if self
            .buffers
            .as_ref()
            .is_none_or(|b| b.capacity_bytes < byte_len)
        {
            let buffer = |label, size, usage| {
                renderer.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            self.buffers = Some(Buffers {
                output: buffer(
                    "starroom-denoise-output",
                    byte_len,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                ),
                staging: buffer(
                    "starroom-denoise-final-readback",
                    byte_len + 32,
                    wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                ),
                parameters: buffer(
                    "starroom-denoise-parameters",
                    32,
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                ),
                weights: buffer(
                    "starroom-denoise-weights",
                    289 * 4,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                ),
                timing: renderer.timing_resources(),
                capacity_bytes: byte_len,
                identity: None,
            });
        }
        let buffers = self
            .buffers
            .as_mut()
            .ok_or_else(|| GpuError::Validation("denoise buffers unavailable".into()))?;
        let hit = buffers.identity == Some(identity);
        if !hit {
            renderer
                .queue
                .write_buffer(&buffers.parameters, 0, bytemuck::bytes_of(&parameters));
            renderer
                .queue
                .write_buffer(&buffers.weights, 0, bytemuck::cast_slice(&weights));
            let group = renderer
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("starroom-denoise-group"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: buffers.output.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: buffers.parameters.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: buffers.weights.as_entire_binding(),
                        },
                    ],
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("starroom-denoise-pass"),
                    timestamp_writes: buffers.timing.as_ref().map(GpuTimingResources::writes),
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(parameters.shape[2].div_ceil(64), 1, 1);
            }
            buffers.identity = Some(identity);
        }
        Ok(EncodedDenoise {
            identity,
            output: buffers.output.clone(),
            staging: buffers.staging.clone(),
            timestamps: !hit && buffers.timing.is_some(),
            cache_hit: hit,
        })
    }

    pub(super) fn resolve_timing(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        staging: &wgpu::Buffer,
        offset: u64,
    ) {
        if let Some(buffers) = self.buffers.as_ref()
            && let Some(timing) = buffers.timing.as_ref()
        {
            timing.resolve_into(encoder, staging, offset);
        }
    }
}

impl GpuRenderer {
    pub(super) fn encode_denoise(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Buffer,
        creative_identity: u64,
        request: GpuDenoiseRequest,
        byte_len: u64,
    ) -> Result<EncodedDenoise, GpuError> {
        request.prepare((byte_len / 16) as usize)?;
        let mut stage = self.denoise.lock().map_err(|_| GpuError::DeviceLost)?;
        if stage.is_none() {
            *stage = Some(crate::profiling::measure(
                crate::profiling::ProfileStage::GpuDenoiseInitialization,
                0,
                || GpuDenoiseStage::new(self),
            )?);
        }
        stage
            .as_mut()
            .ok_or_else(|| GpuError::Validation("denoise stage unavailable".into()))?
            .encode(self, encoder, input, creative_identity, request, byte_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starroom_detail::{LinearImage, denoise};

    #[test]
    fn real_gpu_denoise_matches_cpu_hdr_borders_and_preserves_one_readback_and_cache() {
        let Ok(renderer) = GpuRenderer::try_new() else {
            eprintln!("GPU adapter unavailable; real Native release gate still required");
            return;
        };
        for (width, height) in [(1, 1), (1, 7), (7, 1), (17, 11), (65, 3)] {
            let pixels: Vec<_> = (0..width * height)
                .map(|index| {
                    let value = [-0.15, 0.0, 0.015, 0.18, 0.7, 1.5, 4.0][index % 7];
                    [
                        value,
                        value * 0.7 + 0.01,
                        value * 0.4 - 0.01,
                        (index % 5) as f32 * 0.25,
                    ]
                })
                .collect();
            let cpu = LinearImage::new(
                width,
                height,
                pixels.iter().flat_map(|p| p[..3].iter().copied()).collect(),
            )
            .unwrap();
            let mut creative = GpuCreativeParameters {
                values: [[0.0; 4]; 23],
            };
            creative.values[18] = [pixels.len() as f32, width as f32, height as f32, 0.0];
            for radius in [0.6, 1.25, 4.0] {
                for (luminance, chroma, high_iso) in [
                    (0.0, 0.0, 0.0),
                    (0.6, 0.0, 0.0),
                    (0.0, 0.7, 0.0),
                    (0.9, 0.8, 1.0),
                ] {
                    let p = DenoiseParameters {
                        radius,
                        luminance,
                        chroma,
                        high_iso,
                        ..Default::default()
                    };
                    let request = GpuDenoiseRequest {
                        width: width as u32,
                        height: height as u32,
                        parameters: p,
                    };
                    let expected = denoise(&cpu, p);
                    let before = renderer.resource_stats();
                    let actual = renderer
                        .apply_creative_with_denoise(
                            &pixels,
                            &creative,
                            &[0.0; 8192],
                            Some("nr-oracle"),
                            request,
                        )
                        .unwrap();
                    assert_eq!(
                        renderer.resource_stats().final_readback_count,
                        before.final_readback_count + 1
                    );
                    for (index, pixel) in actual.iter().enumerate() {
                        assert_eq!(pixel[3].to_bits(), pixels[index][3].to_bits());
                        for (channel, &value) in pixel[..3].iter().enumerate() {
                            let expected = expected.data[index * 3 + channel];
                            assert!(value.is_finite());
                            assert!(
                                (value - expected).abs() <= 2.0e-5 * (1.0 + expected.abs()),
                                "{width}x{height} radius{radius} {luminance}/{chroma}/{high_iso}: {value} vs {expected}"
                            );
                        }
                    }
                    if luminance == 0.0 && chroma == 0.0 && high_iso == 0.0 {
                        assert_eq!(actual, pixels);
                    }
                    let (cached, profile) = crate::profiling::capture(|| {
                        renderer.apply_creative_with_denoise(
                            &pixels,
                            &creative,
                            &[0.0; 8192],
                            Some("nr-oracle"),
                            request,
                        )
                    });
                    assert_eq!(cached.unwrap(), actual);
                    assert_eq!(
                        profile.gpu_hardware_pass_count, 0,
                        "cached outputs must not invent GPU executions"
                    );
                }
            }
        }
        assert_eq!(renderer.resource_stats().pipeline_create_count, 3);
        let bad = GpuDenoiseRequest {
            width: 1,
            height: 2,
            parameters: DenoiseParameters::default(),
        };
        let creative = GpuCreativeParameters {
            values: [[0.0; 4]; 23],
        };
        assert!(matches!(
            renderer.apply_creative_with_denoise(&[[0.1; 4]], &creative, &[0.0; 8192], None, bad),
            Err(GpuError::InvalidPixels)
        ));
    }
}
