//! M12 wgpu acceleration backend.
//!
//! The CPU graph remains Starroom's image-quality oracle.  This module owns the explicit GPU
//! lifecycle and an exposure compute node whose arithmetic is compared with the CPU oracle
//! before it is eligible for preview scheduling.  The resource contract is linear Rec.2020 D65
//! RGBA16Float; readback buffers use f32 only at the CPU/GPU boundary so comparisons do not hide
//! half-float quantisation errors.

use bytemuck::{Pod, Zeroable};
use serde::Serialize;
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
    time::Instant,
};
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

const CREATIVE_WGSL: &str = r#"
struct CreativeParameters { values: array<vec4<f32>, 20>, };
@group(0) @binding(0) var<storage, read> source: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: CreativeParameters;
@group(0) @binding(3) var<storage, read> curves: array<f32>;

fn finite(v: f32) -> f32 { if (v != v || abs(v) > 3.4028234e38) { return 0.0; } return v; }
fn sstep(a:f32,b:f32,v:f32)->f32 { let t=clamp((v-a)/max(b-a,1e-7),0.0,1.0); return t*t*(3.0-2.0*t); }
fn xyz(rgb:vec3<f32>)->vec3<f32>{return vec3<f32>(.63695806*rgb.r+.1446169*rgb.g+.16888098*rgb.b,.2627002*rgb.r+.67799807*rgb.g+.05930172*rgb.b,.028072693*rgb.g+1.0609851*rgb.b);}
fn lab(rgb:vec3<f32>)->vec3<f32>{let q=xyz(rgb);let l=pow(abs(.818933*q.x+.36186674*q.y-.12885971*q.z),1.0/3.0)*sign(.818933*q.x+.36186674*q.y-.12885971*q.z);let m=pow(abs(.032984544*q.x+.9293119*q.y+.03614564*q.z),1.0/3.0)*sign(.032984544*q.x+.9293119*q.y+.03614564*q.z);let s=pow(abs(.0482003*q.x+.26436627*q.y+.6338517*q.z),1.0/3.0)*sign(.0482003*q.x+.26436627*q.y+.6338517*q.z);return vec3<f32>(.21045426*l+.7936178*m-.004072047*s,1.9779985*l-2.4285922*m+.4505937*s,.025904037*l+.78277177*m-.80867577*s);}
fn rgb(v:vec3<f32>)->vec3<f32>{let lp=v.x+.39633778*v.y+.21580376*v.z;let mp=v.x-.105561346*v.y-.06385417*v.z;let sp=v.x-.08948418*v.y-1.2914855*v.z;let l=lp*lp*lp;let m=mp*mp*mp;let s=sp*sp*sp;let x=1.227014*l-.5578*m+.28125614*s;let y=-.04058018*l+1.1122569*m-.07167668*s;let z=-.07638129*l-.42148197*m+1.5861632*s;return vec3<f32>(1.7166512*x-.35567078*y-.2533663*z,-.6666843*x+1.6164812*y+.015768546*z,.017639857*x-.042770613*y+.9421031*z);}
fn tone(y0:f32)->f32{var y=max(y0,0.0)*exp2(clamp(p.values[0].x,-5.0,5.0));let sw=sstep(.004,.012,y)*(1.0-sstep(.06,.18,y));let bw=1.0-sstep(0.0,.11,y);let hw=sstep(.34,.62,y)*(1.0-.25*sstep(1.10,8.0,y));let ww=sstep(.72,1.02,y);let sh=clamp(p.values[0].w,-1.0,1.0);if(sh>=0.0){y+=sh*sw*(.24+.18*sqrt(y))*(1.0-min(y,1.0));}else{y*=1.0+sh*sw*.72;}let hi=clamp(p.values[0].z,-1.0,1.0);if(hi<0.0){let strength=-hi*hw;let fr=pow(.000152+y,1.22+strength*1.45);let mapped=pow(fr/(.84+fr),1.0+strength*.55);let shoulder=select(y,.1845+max(mapped-.1845,0.0),y>.1845);y+= (shoulder-y)*strength;}else{y+=hi*hw*(1.0-min(y,1.0))*.22;}let bl=clamp(p.values[1].x,-1.0,1.0);if(bl>=0.0){y+=bl*bw*.055;}else{y*=1.0+bl*bw*.82;}let wh=clamp(p.values[1].y,-1.0,1.0);if(wh>=0.0){y+=wh*ww*(.10+.10*min(y,1.0));}else{y*=1.0+wh*ww*.48;}let c=clamp(p.values[0].y,-1.0,1.0);if(abs(c)>1e-7){y=.18*exp2(log2(max(y,1e-6)/.18)*(1.0+c*.62));}return max(finite(y),0.0);}
fn curve(channel:u32,v:f32)->f32{let base=channel*1024u;if(v<=0.0){return curves[base]+v*(curves[base+1u]-curves[base])*1023.0;}if(v>=1.0){return curves[base+1023u]+(v-1.0)*(curves[base+1023u]-curves[base+1022u])*1023.0;}let x=v*1023.0;let i=u32(floor(x));return mix(curves[base+i],curves[base+min(i+1u,1023u)],fract(x));}
fn tone_lut(v:f32)->f32{if(v<=0.0){return curves[4096u];}let x=clamp((log2(v)+24.0)/40.0,0.0,1.0)*4094.0+1.0;let i=u32(floor(x));return mix(curves[4096u+i],curves[4096u+min(i+1u,4095u)],fract(x));}
fn hue_dist(a:f32,b:f32)->f32{let d=abs(a-b);return min(d,360.0-d);}
fn mixer(c:vec3<f32>)->vec3<f32>{var l=lab(c);var h=degrees(atan2(l.z,l.y));if(h<0.0){h+=360.0;}let chroma=length(l.yz);if(chroma<1e-4){return c;}let centers=array<f32,8>(25.0,55.0,95.0,145.0,195.0,250.0,300.0,335.0);var total=0.0;var dh=0.0;var dc=0.0;var dl=0.0;let width=clamp(p.values[4].x,30.0,80.0);for(var i:u32=0u;i<8u;i++){let w=1.0-sstep(width*.42,width,hue_dist(h,centers[i]));let a=p.values[5u+i];total+=w;dh+=clamp(a.x,-30.0,30.0)*w;dc+=clamp(a.y,-1.0,1.0)*w;dl+=clamp(a.z,-1.0,1.0)*w;}if(total>1e-7){let nh=radians(h+dh/total);let nc=max(chroma*(1.0+dc/total*.75),0.0);l=vec3<f32>(l.x+dl/total*.16,nc*cos(nh),nc*sin(nh));}return rgb(l);}
fn wheel(l:ptr<function,vec3<f32>>,w:vec4<f32>,weight:f32,amount:f32){let a=radians(w.x);(*l).y+=cos(a)*clamp(w.y,-1.0,1.0)*.12*weight*amount;(*l).z+=sin(a)*clamp(w.y,-1.0,1.0)*.12*weight*amount;(*l).x+=clamp(w.z,-1.0,1.0)*.12*weight*amount;}
fn grade(c:vec3<f32>)->vec3<f32>{var l=lab(c);let q=p.values[17];let amount=clamp(q.z,0.0,1.0);let shift=clamp(q.x,-1.0,1.0)*.12;let overlap=.08+clamp(q.y,0.0,1.0)*.18;let sw=1.0-sstep(.42+shift-overlap,.42+shift+overlap,l.x);let hw=sstep(.58+shift-overlap,.58+shift+overlap,l.x);let mw=clamp(1.0-max(sw,hw),0.0,1.0);let sum=max(sw+mw+hw,1e-7);wheel(&l,p.values[13],sw/sum,amount);wheel(&l,p.values[14],mw/sum,amount);wheel(&l,p.values[15],hw/sum,amount);wheel(&l,p.values[16],1.0,amount);l.x=max(l.x,0.0);return rgb(l);}
@compute @workgroup_size(64) fn creative_main(@builtin(global_invocation_id) id:vec3<u32>){let n=u32(p.values[18].x);if(id.x>=n){return;}var c=source[id.x].rgb;var l=lab(c);l.z+=clamp(p.values[1].z,-1.0,1.0)*.035;l.y+=clamp(p.values[1].w,-1.0,1.0)*.025;let ch=length(l.yz);let scale=max(1.0+clamp(p.values[2].y,-1.0,1.0)*.85+clamp(p.values[2].x,-1.0,1.0)*(1.0-clamp(ch/.32,0.0,1.0))*.65,0.0);l.y*=scale;l.z*=scale;c=rgb(l);let lum=max(dot(c,vec3<f32>(.2627,.6780,.0593)),0.0);let mapped=tone_lut(lum);c=select(vec3<f32>(mapped),c*(mapped/max(lum,1e-7)),lum>1e-7);if(p.values[19].x>0.5){c=vec3<f32>(curve(1u,curve(0u,c.r)),curve(2u,curve(0u,c.g)),curve(3u,curve(0u,c.b)));}if(p.values[19].y>0.5){c=mixer(c);}if(p.values[19].z>0.5){c=grade(c);}if(p.values[19].w>0.5){let width=max(p.values[18].y,1.0);let height=max(p.values[18].z,1.0);let px=f32(id.x%u32(width));let py=f32(id.x/u32(width));let dx=(px-(width-1.0)*.5)/max((width-1.0)*.5,1.0);let dy=(py-(height-1.0)*.5)/max((height-1.0)*.5,1.0);let roundness=(clamp(p.values[3].x,-1.0,1.0)+1.0)*.5;let aspect=width/height;let aspect_correction=1.0+(max(aspect,1.0)-1.0)*(1.0-roundness);let radius=sqrt((dx*aspect_correction)*(dx*aspect_correction)+dy*dy);let edge=clamp((radius-clamp(p.values[2].w,0.0,1.0))/max(abs(p.values[3].y),.02),0.0,1.0);let edge_weight=edge*edge*(3.0-2.0*edge);let vignette_luminance=max(dot(c,vec3<f32>(.2627,.6780,.0593)),0.0);let highlight_weight=clamp((vignette_luminance-.55)/1.45,0.0,1.0);let protect_weight=1.0-clamp(p.values[3].z,0.0,1.0)*highlight_weight*highlight_weight*(3.0-2.0*highlight_weight);c*=exp2(-clamp(p.values[2].z,-1.0,1.0)*2.0*edge_weight*protect_weight);}output[id.x]=vec4<f32>(finite(c.r),finite(c.g),finite(c.b),source[id.x].a);}
"#;

fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

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

/// Counters come from the production renderer, not a benchmark shim. They make the one-upload /
/// one-readback contract and resource reuse observable to both tests and the desktop profiler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuResourceStats {
    pub source_upload_count: u64,
    pub final_readback_count: u64,
    pub resource_reuse_count: u64,
    pub pipeline_create_count: u64,
    pub source_texture_hit: u64,
    pub source_texture_miss: u64,
    pub working_texture_hit: u64,
    pub working_texture_miss: u64,
    pub gpu_stage_cache_hit: u64,
    pub gpu_stage_cache_miss: u64,
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

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct GpuCreativeParameters {
    pub values: [[f32; 4]; 20],
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
    creative_pipeline: wgpu::ComputePipeline,
    exposure_layout: wgpu::BindGroupLayout,
    creative_layout: wgpu::BindGroupLayout,
    device_lost: bool,
    resources: Mutex<Option<GpuBufferResources>>,
    stats: Mutex<GpuResourceStats>,
}

struct GpuBufferResources {
    capacity_bytes: u64,
    source_fingerprint: u64,
    creative_fingerprint: u64,
    source: wgpu::Buffer,
    _working: wgpu::Buffer,
    output: wgpu::Buffer,
    staging: wgpu::Buffer,
    parameters: wgpu::Buffer,
    /// Reserved by the fused creative path. Keeping these allocations with the frame resources
    /// prevents curve or mask edits from rebuilding large GPU storage.
    curve_lut: wgpu::Buffer,
    _mask: wgpu::Buffer,
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
        let creative_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("starroom-creative-layout"),
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
        let creative_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("starroom-creative-pipeline-layout"),
                bind_group_layouts: &[Some(&creative_layout)],
                immediate_size: 0,
            });
        let creative_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("starroom-fused-creative-wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(CREATIVE_WGSL)),
        });
        let creative_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("starroom-fused-creative-pipeline"),
            layout: Some(&creative_pipeline_layout),
            module: &creative_shader,
            entry_point: Some("creative_main"),
            compilation_options: Default::default(),
            cache: None,
        });
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
            creative_pipeline,
            exposure_layout,
            creative_layout,
            device_lost: false,
            resources: Mutex::new(None),
            stats: Mutex::new(GpuResourceStats {
                pipeline_create_count: 2,
                ..Default::default()
            }),
        })
    }

    pub fn status(&self) -> &GpuStatus {
        &self.status
    }

    pub fn resource_stats(&self) -> GpuResourceStats {
        *self.stats.lock().expect("GPU resource statistics")
    }

    pub fn reset_resource_stats(&self) {
        let pipeline_create_count = self
            .stats
            .lock()
            .expect("GPU resource statistics")
            .pipeline_create_count;
        *self.stats.lock().expect("GPU resource statistics") = GpuResourceStats {
            pipeline_create_count,
            ..Default::default()
        };
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
        let mut resources = self.resources.lock().map_err(|_| GpuError::DeviceLost)?;
        let must_allocate = resources
            .as_ref()
            .is_none_or(|resources| resources.capacity_bytes < byte_len);
        if must_allocate {
            let storage = |label, usage| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: byte_len,
                    usage,
                    mapped_at_creation: false,
                })
            };
            *resources = Some(GpuBufferResources {
                capacity_bytes: byte_len,
                source_fingerprint: 0,
                creative_fingerprint: 0,
                source: storage(
                    "starroom-source-linear-rec2020",
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                ),
                _working: storage(
                    "starroom-working-ping",
                    wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                ),
                output: storage(
                    "starroom-working-pong",
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                ),
                staging: storage(
                    "starroom-final-readback",
                    wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                ),
                parameters: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("starroom-creative-uniforms"),
                    size: 512,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                curve_lut: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("starroom-curve-luts"),
                    size: 8 * 1024 * 4,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                _mask: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("starroom-mask-r16float-storage"),
                    size: (byte_len / 4).max(4),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            });
        } else {
            self.stats
                .lock()
                .expect("GPU resource statistics")
                .resource_reuse_count += 1;
        }
        let resources = resources.as_mut().expect("allocated GPU resources");
        let fingerprint = pixel_fingerprint(pixels);
        if must_allocate || resources.source_fingerprint != fingerprint {
            self.queue
                .write_buffer(&resources.source, 0, bytemuck::cast_slice(pixels));
            resources.source_fingerprint = fingerprint;
            self.stats
                .lock()
                .expect("GPU resource statistics")
                .source_upload_count += 1;
        }
        let parameters = ExposureParameters {
            exposure_ev,
            pixel_count: pixels.len() as u32,
            padding0: 0,
            padding1: 0,
        };
        self.queue
            .write_buffer(&resources.parameters, 0, bytemuck::bytes_of(&parameters));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("starroom-m12-exposure-bind-group"),
            layout: &self.exposure_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: resources.source.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: resources.output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: resources.parameters.as_entire_binding(),
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
        encoder.copy_buffer_to_buffer(&resources.output, 0, &resources.staging, 0, byte_len);
        self.queue.submit(Some(encoder.finish()));
        let slice = resources.staging.slice(..byte_len);
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
        resources.staging.unmap();
        self.stats
            .lock()
            .expect("GPU resource statistics")
            .final_readback_count += 1;
        if !result.iter().flatten().all(|value| value.is_finite()) {
            return Err(GpuError::InvalidPixels);
        }
        Ok(result)
    }

    /// Executes WB, tone, four curves, OKLCh mixer, four-way grading and vignette as one GPU
    /// pass. Source storage, output/readback storage, uniforms and LUTs persist across frames.
    pub fn apply_creative(
        &self,
        pixels: &[[f32; 4]],
        parameters: &GpuCreativeParameters,
        curve_luts: &[f32; 8192],
        stage_identity: Option<&str>,
    ) -> Result<Vec<[f32; 4]>, GpuError> {
        if self.device_lost
            || pixels.is_empty()
            || !pixels.iter().flatten().all(|value| value.is_finite())
            || !parameters
                .values
                .iter()
                .flatten()
                .all(|value| value.is_finite())
            || !curve_luts.iter().all(|value| value.is_finite())
        {
            return Err(GpuError::InvalidPixels);
        }
        let byte_len = std::mem::size_of_val(pixels) as u64;
        if byte_len > self.device.limits().max_storage_buffer_binding_size {
            return Err(GpuError::Unsupported(
                "creative buffer exceeds adapter storage binding limit".into(),
            ));
        }
        let mut resources = self.resources.lock().map_err(|_| GpuError::DeviceLost)?;
        let must_allocate = resources
            .as_ref()
            .is_none_or(|resources| resources.capacity_bytes < byte_len);
        if must_allocate {
            let storage = |label, size, usage| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            *resources = Some(GpuBufferResources {
                capacity_bytes: byte_len,
                source_fingerprint: 0,
                creative_fingerprint: 0,
                source: storage(
                    "starroom-source-linear-rec2020",
                    byte_len,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                ),
                _working: storage(
                    "starroom-working-ping",
                    byte_len,
                    wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                ),
                output: storage(
                    "starroom-working-pong",
                    byte_len,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                ),
                staging: storage(
                    "starroom-final-readback",
                    byte_len,
                    wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                ),
                parameters: storage(
                    "starroom-creative-uniforms",
                    512,
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                ),
                curve_lut: storage(
                    "starroom-curve-luts",
                    8 * 1024 * 4,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                ),
                _mask: storage(
                    "starroom-mask-r16float-storage",
                    (byte_len / 4).max(4),
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                ),
            });
        } else {
            self.stats
                .lock()
                .expect("GPU resource statistics")
                .resource_reuse_count += 1;
        }
        let resources = resources.as_mut().expect("allocated GPU resources");
        let fingerprint = pixel_fingerprint(pixels);
        if must_allocate || resources.source_fingerprint != fingerprint {
            let upload_started = Instant::now();
            self.queue
                .write_buffer(&resources.source, 0, bytemuck::cast_slice(pixels));
            resources.source_fingerprint = fingerprint;
            let mut stats = self.stats.lock().expect("GPU resource statistics");
            stats.source_upload_count += 1;
            stats.source_texture_miss += 1;
            stats.gpu_stage_cache_miss += 1;
            crate::profiling::record_gpu_cache_delta(0, 1, 0, 0);
            crate::profiling::record_gpu_transfer(
                u64::try_from(upload_started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                0,
            );
        } else {
            let mut stats = self.stats.lock().expect("GPU resource statistics");
            stats.source_texture_hit += 1;
            stats.gpu_stage_cache_hit += 1;
            crate::profiling::record_gpu_cache_delta(1, 0, 0, 0);
        }
        let creative_fingerprint =
            creative_fingerprint(fingerprint, parameters, curve_luts, stage_identity);
        let creative_hit = !must_allocate && resources.creative_fingerprint == creative_fingerprint;
        if creative_hit {
            let mut stats = self.stats.lock().expect("GPU resource statistics");
            stats.working_texture_hit += 1;
            stats.gpu_stage_cache_hit += 1;
            crate::profiling::record_gpu_cache_delta(0, 0, 1, 0);
        } else {
            self.queue
                .write_buffer(&resources.parameters, 0, bytemuck::bytes_of(parameters));
            self.queue
                .write_buffer(&resources.curve_lut, 0, bytemuck::cast_slice(curve_luts));
            resources.creative_fingerprint = creative_fingerprint;
            let mut stats = self.stats.lock().expect("GPU resource statistics");
            stats.working_texture_miss += 1;
            stats.gpu_stage_cache_miss += 1;
            crate::profiling::record_gpu_cache_delta(0, 0, 0, 1);
        }
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("starroom-fused-creative-bind-group"),
            layout: &self.creative_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: resources.source.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: resources.output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: resources.parameters.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: resources.curve_lut.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("starroom-fused-creative-encoder"),
            });
        if !creative_hit {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("starroom-fused-creative-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.creative_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((pixels.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&resources.output, 0, &resources.staging, 0, byte_len);
        self.queue.submit(Some(encoder.finish()));
        let readback_started = Instant::now();
        let slice = resources.staging.slice(..byte_len);
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
        resources.staging.unmap();
        crate::profiling::record_gpu_transfer(
            0,
            u64::try_from(readback_started.elapsed().as_nanos()).unwrap_or(u64::MAX),
        );
        self.stats
            .lock()
            .expect("GPU resource statistics")
            .final_readback_count += 1;
        if !result.iter().flatten().all(|value| value.is_finite()) {
            return Err(GpuError::InvalidPixels);
        }
        Ok(result)
    }
}

fn pixel_fingerprint(pixels: &[[f32; 4]]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for component in bytemuck::cast_slice::<[f32; 4], u32>(pixels) {
        component.hash(&mut hasher);
    }
    hasher.finish()
}

fn creative_fingerprint(
    source_fingerprint: u64,
    parameters: &GpuCreativeParameters,
    curve_luts: &[f32; 8192],
    stage_identity: Option<&str>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    source_fingerprint.hash(&mut hasher);
    if let Some(identity) = stage_identity {
        identity.hash(&mut hasher);
    } else {
        bytemuck::cast_slice::<GpuCreativeParameters, u32>(std::slice::from_ref(parameters))
            .hash(&mut hasher);
        bytemuck::cast_slice::<f32, u32>(curve_luts).hash(&mut hasher);
    }
    hasher.finish()
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

    #[test]
    fn persistent_creative_resources_reuse_source_and_pipeline_between_slider_frames() {
        let Ok(renderer) = GpuRenderer::try_new() else {
            return;
        };
        let pixels = vec![[0.18, 0.2, 0.22, 1.0]; 128];
        let mut luts = [0.0; 8192];
        for channel in 0..4 {
            for sample in 0..1024 {
                luts[channel * 1024 + sample] = sample as f32 / 1023.0;
            }
        }
        for sample in 0..4096 {
            luts[4096 + sample] = if sample == 0 {
                0.0
            } else {
                2.0_f32.powf(-24.0 + 40.0 * (sample - 1) as f32 / 4094.0)
            };
        }
        let mut parameters = GpuCreativeParameters {
            values: [[0.0; 4]; 20],
        };
        parameters.values[18] = [pixels.len() as f32, pixels.len() as f32, 1.0, 0.0];
        renderer.reset_resource_stats();
        renderer
            .apply_creative(&pixels, &parameters, &luts, Some("creative-a"))
            .expect("first frame");
        parameters.values[0][0] = 0.25;
        renderer
            .apply_creative(&pixels, &parameters, &luts, Some("creative-b"))
            .expect("slider frame");
        renderer
            .apply_creative(&pixels, &parameters, &luts, Some("creative-b"))
            .expect("display-only frame reuses working texture");
        let stats = renderer.resource_stats();
        assert_eq!(stats.source_upload_count, 1);
        assert_eq!(stats.final_readback_count, 3);
        assert_eq!(stats.pipeline_create_count, 2);
        assert!(stats.resource_reuse_count >= 1);
        assert!(stats.source_texture_hit >= 2);
        assert_eq!(stats.working_texture_hit, 1);
        assert!(stats.gpu_stage_cache_hit >= 3);
    }

    #[test]
    fn larger_frame_reallocates_without_reusing_freed_storage() {
        let Ok(renderer) = GpuRenderer::try_new() else {
            return;
        };
        let small = vec![[0.1, 0.2, 0.3, 1.0]; 64];
        let large = vec![[0.1, 0.2, 0.3, 1.0]; 256];
        renderer.apply_exposure(&small, 0.0).expect("small frame");
        renderer.apply_exposure(&large, 0.0).expect("resized frame");
        assert_eq!(renderer.resource_stats().source_upload_count, 2);
    }
}
