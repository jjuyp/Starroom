//! M12 wgpu acceleration backend.
//!
//! The CPU graph remains Starroom's image-quality oracle.  This module owns the explicit GPU
//! lifecycle and an exposure compute node whose arithmetic is compared with the CPU oracle
//! before it is eligible for preview scheduling.  The resource contract is linear Rec.2020 D65
//! RGBA16Float; readback buffers use f32 only at the CPU/GPU boundary so comparisons do not hide
//! half-float quantisation errors.

use bytemuck::{Pod, Zeroable};
use serde::Serialize;
use std::{borrow::Cow, sync::Arc};
use thiserror::Error;

pub const GPU_WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const GPU_MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

const EXPOSURE_WGSL: &str = r#"
struct Parameters {
  exposure_ev: f32,
  pixel_count: u32,
  _padding0: u32,
  _padding1: u32,
};

@group(0) @binding(0) var<storage, read> input_pixels: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output_pixels: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> parameters: Parameters;

fn finite_or_zero(value: f32) -> f32 {
  // WGSL deliberately has no isFinite builtin. NaN is the only value unequal to itself, and
  // |value| above the largest finite f32 catches both infinity signs without clamping HDR data.
  if (value != value || abs(value) > 3.4028234e38) { return 0.0; }
  return value;
}

@compute @workgroup_size(64)
fn exposure_main(@builtin(global_invocation_id) id: vec3<u32>) {
  let index = id.x;
  if (index >= parameters.pixel_count) { return; }
  let source = input_pixels[index];
  let gain = exp2(parameters.exposure_ev);
  output_pixels[index] = vec4<f32>(
    finite_or_zero(source.r * gain),
    finite_or_zero(source.g * gain),
    finite_or_zero(source.b * gain),
    finite_or_zero(source.a)
  );
}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuBackendKind {
    Dx12,
    Other,
    CpuFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    pub backend: GpuBackendKind,
    pub adapter_name: Option<String>,
    pub reason: Option<String>,
}

/// Probes the acceleration backend without rendering image pixels. The desktop UI uses this to
/// distinguish DX12/other wgpu acceleration from a deliberately reported CPU fallback reason.
pub fn probe_gpu_status(prefer_gpu: bool) -> GpuStatus {
    if !prefer_gpu {
        return GpuStatus::cpu_fallback("GPU preview is disabled by request");
    }
    match GpuRenderer::try_new() {
        Ok(renderer) => renderer.status().clone(),
        Err(error) => GpuStatus::cpu_fallback(error.to_string()),
    }
}

impl GpuStatus {
    pub fn cpu_fallback(reason: impl Into<String>) -> Self {
        Self {
            backend: GpuBackendKind::CpuFallback,
            adapter_name: None,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("no compatible wgpu adapter was found")]
    NoCompatibleAdapter,
    #[error("wgpu device creation failed: {0}")]
    Device(String),
    #[error("GPU feature or resource size is unsupported on this adapter: {0}")]
    Unsupported(String),
    #[error("GPU ran out of memory; preview must use the CPU reference backend")]
    OutOfMemory,
    #[error("WGSL shader compilation or validation failed: {0}")]
    Shader(String),
    #[error("wgpu validation failed: {0}")]
    Validation(String),
    #[error("GPU pixel buffer is empty, mismatched, or contains non-finite values")]
    InvalidPixels,
    #[error("GPU device was lost; preview must use the CPU reference backend")]
    DeviceLost,
    #[error("GPU readback failed: {0}")]
    Readback(String),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ExposureParameters {
    exposure_ev: f32,
    pixel_count: u32,
    padding0: u32,
    padding1: u32,
}

/// Owns the wgpu instance, adapter, device, queue, shader module, pipeline cache boundary and
/// bind group layouts. It deliberately exposes only typed operations, never wgpu objects to UI.
pub struct GpuRenderer {
    // wgpu resources are tied to this explicit instance lifetime. Keeping it here also makes
    // adapter/backend diagnostics valid for the complete renderer lifetime.
    _instance: wgpu::Instance,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    status: GpuStatus,
    exposure_pipeline: wgpu::ComputePipeline,
    exposure_layout: wgpu::BindGroupLayout,
    device_lost: bool,
}

impl GpuRenderer {
    pub fn try_new() -> Result<Self, GpuError> {
        pollster::block_on(Self::initialize())
    }

    async fn initialize() -> Result<Self, GpuError> {
        // Windows is authoritative for M12. Try DX12 first; a non-DX12 adapter remains an
        // explicitly labelled development fallback rather than a silent semantic substitution.
        let mut dx12_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        dx12_descriptor.backends = wgpu::Backends::DX12;
        let dx12 = wgpu::Instance::new(dx12_descriptor);
        let dx12_adapter = dx12
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .ok();
        let (instance, adapter, backend) = if let Some(adapter) = dx12_adapter {
            (dx12, adapter, GpuBackendKind::Dx12)
        } else {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|_| GpuError::NoCompatibleAdapter)?;
            (instance, adapter, GpuBackendKind::Other)
        };
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("starroom-m12-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|error| GpuError::Device(error.to_string()))?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let exposure_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("starroom-m12-exposure-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("starroom-m12-exposure-pipeline-layout"),
            bind_group_layouts: &[Some(&exposure_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("starroom-m12-exposure-wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(EXPOSURE_WGSL)),
        });
        let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let exposure_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("starroom-m12-exposure-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("exposure_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(error) = validation_scope.pop().await {
            return Err(GpuError::Shader(error.to_string()));
        }
        Ok(Self {
            _instance: instance,
            device,
            queue,
            status: GpuStatus {
                backend,
                adapter_name: Some(info.name),
                reason: None,
            },
            exposure_pipeline,
            exposure_layout,
            device_lost: false,
        })
    }

    pub fn status(&self) -> &GpuStatus {
        &self.status
    }

    pub fn mark_device_lost(&mut self) {
        self.device_lost = true;
    }

    /// Explicit test/runtime hook for an allocation failure observed by a scheduler. The caller
    /// must surface the CPU fallback status rather than attempting a hidden retry.
    pub fn mark_out_of_memory(&mut self) {
        self.device_lost = true;
    }

    /// Creates the canonical RGBA16Float / R16Float resources used by preview and masks. Their
    /// ownership is explicit, so tile/cache eviction can release GPU memory without touching CPU
    /// reference buffers.
    pub fn create_texture_set(&self, width: u32, height: u32) -> Result<GpuTextureSet, GpuError> {
        if width == 0 || height == 0 {
            return Err(GpuError::InvalidPixels);
        }
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texels = u64::from(width) * u64::from(height);
        if texels > u64::from(self.device.limits().max_texture_dimension_2d).pow(2) {
            return Err(GpuError::Unsupported(
                "texture exceeds adapter dimension limits".into(),
            ));
        }
        let image = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("starroom-m12-linear-rec2020"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: GPU_WORKING_FORMAT,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let mask = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("starroom-m12-mask-r16float"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: GPU_MASK_FORMAT,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        Ok(GpuTextureSet {
            image,
            mask,
            width,
            height,
        })
    }

    /// GPU exposure node. The same unbounded linear-light formula is defined by
    /// `apply_exposure_reference`; the finite guard is a stage-boundary contract, not clipping.
    pub fn apply_exposure(
        &self,
        pixels: &[[f32; 4]],
        exposure_ev: f32,
    ) -> Result<Vec<[f32; 4]>, GpuError> {
        if self.device_lost {
            return Err(GpuError::DeviceLost);
        }
        if pixels.is_empty()
            || !exposure_ev.is_finite()
            || !pixels.iter().flatten().all(|value| value.is_finite())
        {
            return Err(GpuError::InvalidPixels);
        }
        let byte_len = std::mem::size_of_val(pixels) as u64;
        if byte_len > self.device.limits().max_storage_buffer_binding_size {
            return Err(GpuError::Unsupported(
                "exposure buffer exceeds adapter storage binding limit".into(),
            ));
        }
        let input = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("starroom-m12-exposure-input"),
            size: byte_len,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("starroom-m12-exposure-output"),
            size: byte_len,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("starroom-m12-exposure-readback"),
            size: byte_len,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let parameters = ExposureParameters {
            exposure_ev,
            pixel_count: pixels.len() as u32,
            padding0: 0,
            padding1: 0,
        };
        let parameter_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("starroom-m12-exposure-parameters"),
            size: std::mem::size_of::<ExposureParameters>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&input, 0, bytemuck::cast_slice(pixels));
        self.queue
            .write_buffer(&parameter_buffer, 0, bytemuck::bytes_of(&parameters));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("starroom-m12-exposure-bind-group"),
            layout: &self.exposure_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: parameter_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("starroom-m12-exposure-encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("starroom-m12-exposure-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.exposure_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((pixels.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, byte_len);
        self.queue.submit(Some(encoder.finish()));
        let slice = staging.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        receiver
            .recv()
            .map_err(|error| GpuError::Readback(error.to_string()))?
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        let data = slice
            .get_mapped_range()
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        let result = bytemuck::cast_slice::<u8, [f32; 4]>(&data).to_vec();
        drop(data);
        staging.unmap();
        if !result.iter().flatten().all(|value| value.is_finite()) {
            return Err(GpuError::InvalidPixels);
        }
        Ok(result)
    }
}

pub struct GpuTextureSet {
    pub image: wgpu::Texture,
    pub mask: wgpu::Texture,
    pub width: u32,
    pub height: u32,
}

/// CPU oracle for the M12 exposure shader. Scene-linear values remain unbounded; only NaN/Inf is
/// rejected at the public stage boundary.
pub fn apply_exposure_reference(
    pixels: &[[f32; 4]],
    exposure_ev: f32,
) -> Result<Vec<[f32; 4]>, GpuError> {
    if pixels.is_empty()
        || !exposure_ev.is_finite()
        || !pixels.iter().flatten().all(|value| value.is_finite())
    {
        return Err(GpuError::InvalidPixels);
    }
    let gain = 2.0_f32.powf(exposure_ev);
    let output: Vec<[f32; 4]> = pixels
        .iter()
        .map(|pixel| [pixel[0] * gain, pixel[1] * gain, pixel[2] * gain, pixel[3]])
        .collect();
    if !output.iter().flatten().all(|value| value.is_finite()) {
        return Err(GpuError::InvalidPixels);
    }
    Ok(output)
}

/// Explicit backend selection used by preview scheduling. It never changes export semantics.
pub fn resolve_preview_backend(prefer_gpu: bool, gpu: Result<&GpuRenderer, GpuError>) -> GpuStatus {
    if !prefer_gpu {
        return GpuStatus::cpu_fallback("GPU preview is disabled by request");
    }
    match gpu {
        Ok(renderer) => renderer.status().clone(),
        Err(error) => GpuStatus::cpu_fallback(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_exposure_oracle_preserves_scene_linear_hdr_and_alpha() {
        let output = apply_exposure_reference(&[[0.25, 1.5, 4.0, 0.75]], 1.0).expect("reference");
        assert_eq!(output, vec![[0.5, 3.0, 8.0, 0.75]]);
    }

    #[test]
    fn explicit_cpu_fallback_reports_reason() {
        let status = resolve_preview_backend(true, Err(GpuError::NoCompatibleAdapter));
        assert_eq!(status.backend, GpuBackendKind::CpuFallback);
        assert!(
            status
                .reason
                .as_deref()
                .unwrap_or_default()
                .contains("adapter")
        );
    }

    #[test]
    fn disabled_gpu_preference_never_probes_or_hides_cpu_status() {
        let status = probe_gpu_status(false);
        assert_eq!(status.backend, GpuBackendKind::CpuFallback);
        assert_eq!(
            status.reason.as_deref(),
            Some("GPU preview is disabled by request")
        );
    }

    #[test]
    fn typed_failure_variants_remain_explicit_cpu_fallback_causes() {
        for error in [
            GpuError::DeviceLost,
            GpuError::OutOfMemory,
            GpuError::Unsupported("R16Float storage texture".into()),
        ] {
            let status = resolve_preview_backend(true, Err(error));
            assert_eq!(status.backend, GpuBackendKind::CpuFallback);
            assert!(status.reason.is_some());
        }
    }

    #[test]
    fn gpu_exposure_matches_cpu_when_an_adapter_is_available() {
        // Covers neutral, portrait/skin, landscape, neon, HDR, shadows/highlights, saturation
        // and extremes with one compact deterministic parity corpus. RAW and encoded sources
        // reach this node after their respective input transforms, so the node contract itself is
        // deliberately linear Rec.2020 rather than format-specific.
        let pixels = [
            [0.18, 0.18, 0.18, 1.0],    // neutral
            [0.62, 0.31, 0.22, 1.0],    // portrait / skin
            [0.07, 0.28, 0.12, 1.0],    // landscape shadow
            [1.8, 0.04, 1.2, 1.0],      // neon / high saturation
            [8.0, 2.0, 0.5, 0.8],       // scene-linear HDR highlight
            [0.002, 0.004, 0.008, 1.0], // deep shadow
        ];
        let expected = apply_exposure_reference(&pixels, -2.75).expect("CPU reference");
        match GpuRenderer::try_new() {
            Ok(renderer) => {
                let actual = renderer
                    .apply_exposure(&pixels, -2.75)
                    .expect("GPU exposure");
                for (cpu, gpu) in expected.iter().zip(actual) {
                    for (cpu, gpu) in cpu.iter().zip(gpu) {
                        assert!(
                            (cpu - gpu).abs() <= 2.0e-5,
                            "CPU/GPU parity drift: {cpu} vs {gpu}"
                        );
                    }
                }
            }
            Err(error) => {
                let status = resolve_preview_backend(true, Err(error));
                assert_eq!(status.backend, GpuBackendKind::CpuFallback);
            }
        }
    }
}
