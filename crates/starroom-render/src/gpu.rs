//! M12 wgpu acceleration backend.
//!
//! The CPU graph remains Starroom's image-quality oracle.  This module owns the explicit GPU
//! lifecycle and production fused creative compute whose arithmetic is compared with the CPU
//! oracle. Current compute uses linear Rec.2020 D65 f32 storage buffers, including readback;
//! RGBA16Float/R16Float texture constructors are the migration contract, not a claim that the
//! production presentation chain already stays in those textures.

use bytemuck::{Pod, Zeroable};
use serde::Serialize;
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use thiserror::Error;

pub const GPU_WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const GPU_MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;
const GPU_READBACK_LIMIT: Duration = Duration::from_secs(5);
const FAILURE_OOM: u8 = 1;
const FAILURE_DEVICE_LOST: u8 = 2;
const FAILURE_VALIDATION: u8 = 3;

fn wait_for_mapping<T>(
    receiver: &std::sync::mpsc::Receiver<T>,
    deadline: Instant,
) -> Result<T, GpuError> {
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => GpuError::ReadbackTimeout,
            other => GpuError::Readback(other.to_string()),
        })
}

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
struct CreativeParameters { values: array<vec4<f32>, 23>, };
@group(0) @binding(0) var<storage, read> source: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: CreativeParameters;
@group(0) @binding(3) var<storage, read> curves: array<f32>;

fn finite(v: f32) -> f32 { if (v != v || abs(v) > 3.4028234e38) { return 0.0; } return v; }
fn sstep(a:f32,b:f32,v:f32)->f32 { let t=clamp((v-a)/max(b-a,1e-7),0.0,1.0); return t*t*(3.0-2.0*t); }
fn xyz(rgb:vec3<f32>)->vec3<f32>{return vec3<f32>(.63695806*rgb.r+.1446169*rgb.g+.16888098*rgb.b,.2627002*rgb.r+.67799807*rgb.g+.05930172*rgb.b,.028072693*rgb.g+1.0609851*rgb.b);}
fn lab(rgb:vec3<f32>)->vec3<f32>{let q=xyz(rgb);let l=pow(abs(.818933*q.x+.36186674*q.y-.12885971*q.z),1.0/3.0)*sign(.818933*q.x+.36186674*q.y-.12885971*q.z);let m=pow(abs(.032984544*q.x+.9293119*q.y+.03614564*q.z),1.0/3.0)*sign(.032984544*q.x+.9293119*q.y+.03614564*q.z);let s=pow(abs(.0482003*q.x+.26436627*q.y+.6338517*q.z),1.0/3.0)*sign(.0482003*q.x+.26436627*q.y+.6338517*q.z);return vec3<f32>(.21045426*l+.7936178*m-.004072047*s,1.9779985*l-2.4285922*m+.4505937*s,.025904037*l+.78277177*m-.80867577*s);}
fn rgb(v:vec3<f32>)->vec3<f32>{let lp=v.x+.39633778*v.y+.21580376*v.z;let mp=v.x-.105561346*v.y-.06385417*v.z;let sp=v.x-.08948418*v.y-1.2914855*v.z;let l=lp*lp*lp;let m=mp*mp*mp;let s=sp*sp*sp;let x=1.227014*l-.5578*m+.28125614*s;let y=-.04058018*l+1.1122569*m-.07167668*s;let z=-.07638129*l-.42148197*m+1.5861632*s;return vec3<f32>(1.7166512*x-.35567078*y-.2533663*z,-.6666843*x+1.6164812*y+.015768546*z,.017639857*x-.042770613*y+.9421031*z);}
fn curve(channel:u32,v:f32)->f32{let base=channel*1024u;if(v<=0.0){return curves[base]+v*(curves[base+1u]-curves[base])*1023.0;}if(v>=1.0){return curves[base+1023u]+(v-1.0)*(curves[base+1023u]-curves[base+1022u])*1023.0;}let x=v*1023.0;let i=u32(floor(x));return mix(curves[base+i],curves[base+min(i+1u,1023u)],fract(x));}
fn tone_lut(v:f32)->f32{if(v<=0.0){return curves[4096u];}let x=clamp((log2(v)+24.0)/40.0,0.0,1.0)*4094.0+1.0;let i=u32(floor(x));return mix(curves[4096u+i],curves[4096u+min(i+1u,4095u)],fract(x));}
fn hue_dist(a:f32,b:f32)->f32{let d=abs(a-b);return min(d,360.0-d);}
fn mixer(c:vec3<f32>)->vec3<f32>{var l=lab(c);var h=degrees(atan2(l.z,l.y));if(h<0.0){h+=360.0;}let chroma=length(l.yz);if(chroma<1e-4){return c;}let centers=array<f32,8>(25.0,55.0,95.0,145.0,195.0,250.0,300.0,335.0);var total=0.0;var dh=0.0;var dc=0.0;var dl=0.0;let width=clamp(p.values[4].x,30.0,80.0);for(var i:u32=0u;i<8u;i++){let w=1.0-sstep(width*.42,width,hue_dist(h,centers[i]));let a=p.values[5u+i];total+=w;dh+=clamp(a.x,-30.0,30.0)*w;dc+=clamp(a.y,-1.0,1.0)*w;dl+=clamp(a.z,-1.0,1.0)*w;}if(total>1e-7){let nh=radians(h+dh/total);let nc=max(chroma*(1.0+dc/total*.75),0.0);l=vec3<f32>(l.x+dl/total*.16,nc*cos(nh),nc*sin(nh));}return rgb(l);}
fn wheel(l:ptr<function,vec3<f32>>,w:vec4<f32>,weight:f32,amount:f32){let a=radians(w.x);(*l).y+=cos(a)*clamp(w.y,-1.0,1.0)*.12*weight*amount;(*l).z+=sin(a)*clamp(w.y,-1.0,1.0)*.12*weight*amount;(*l).x+=clamp(w.z,-1.0,1.0)*.12*weight*amount;}
fn grade(c:vec3<f32>)->vec3<f32>{var l=lab(c);let q=p.values[17];let amount=clamp(q.z,0.0,1.0);let shift=clamp(q.x,-1.0,1.0)*.12;let overlap=.08+clamp(q.y,0.0,1.0)*.18;let sw=1.0-sstep(.42+shift-overlap,.42+shift+overlap,l.x);let hw=sstep(.58+shift-overlap,.58+shift+overlap,l.x);let mw=clamp(1.0-max(sw,hw),0.0,1.0);let sum=max(sw+mw+hw,1e-7);wheel(&l,p.values[13],sw/sum,amount);wheel(&l,p.values[14],mw/sum,amount);wheel(&l,p.values[15],hw/sum,amount);wheel(&l,p.values[16],1.0,amount);l.x=max(l.x,0.0);return rgb(l);}
fn protected_chroma(l:vec3<f32>)->vec3<f32>{
    let saturation=clamp(p.values[2].y,-1.0,1.0);let vibrance=clamp(p.values[2].x,-1.0,1.0);
    if(abs(saturation)<=1.1920929e-7 && abs(vibrance)<=1.1920929e-7){return l;}
    let ch=length(l.yz);var h=degrees(atan2(l.z,l.y));if(h<0.0){h+=360.0;}
    let skin=(1.0-sstep(12.0,50.0,hue_dist(h,50.0)))*sstep(.015,.045,ch)*(1.0-sstep(.25,.45,ch))*sstep(.08,.25,l.x)*(1.0-sstep(.9,1.1,l.x));
    let low_chroma=1.0-clamp(ch/.32,0.0,1.0);let protection=1.0-.7*clamp(skin,0.0,1.0);
    let saturation_scale=select(1.0+.85*saturation,1.0+saturation,saturation<0.0);
    let scale=saturation_scale*(1.0+.65*vibrance*low_chroma*protection);
    return vec3<f32>(l.x,l.y*scale,l.z*scale);
}
@compute @workgroup_size(64) fn creative_main(@builtin(global_invocation_id) id:vec3<u32>){
    let n=u32(p.values[18].x);if(id.x>=n){return;}var c=source[id.x].rgb;
    if(p.values[3].w>0.5){c=vec3<f32>(dot(p.values[20].xyz,c),dot(p.values[21].xyz,c),dot(p.values[22].xyz,c));}
    if(abs(p.values[2].x)>1.1920929e-7 || abs(p.values[2].y)>1.1920929e-7){c=rgb(protected_chroma(lab(c)));}
    if(p.values[4].y>0.5){
        let lum=max(dot(c,vec3<f32>(.2627,.6780,.0593)),0.0);let mapped=tone_lut(lum);
        c=select(vec3<f32>(mapped),c*(mapped/max(lum,1e-7)),lum>1e-7);
    }
    if(p.values[19].x>0.5){c=vec3<f32>(curve(1u,curve(0u,c.r)),curve(2u,curve(0u,c.g)),curve(3u,curve(0u,c.b)));}
    if(p.values[19].y>0.5){c=mixer(c);}if(p.values[19].z>0.5){c=grade(c);}
    if(p.values[19].w>0.5){
        let width=max(p.values[18].y,1.0);let height=max(p.values[18].z,1.0);let px=f32(id.x%u32(width));let py=f32(id.x/u32(width));
        let dx=(px-(width-1.0)*.5)/max((width-1.0)*.5,1.0);let dy=(py-(height-1.0)*.5)/max((height-1.0)*.5,1.0);
        let roundness=(clamp(p.values[3].x,-1.0,1.0)+1.0)*.5;let aspect=width/height;let aspect_correction=1.0+(max(aspect,1.0)-1.0)*(1.0-roundness);
        let radius=sqrt((dx*aspect_correction)*(dx*aspect_correction)+dy*dy);let edge=clamp((radius-clamp(p.values[2].w,0.0,1.0))/max(abs(p.values[3].y),.02),0.0,1.0);
        let edge_weight=edge*edge*(3.0-2.0*edge);let vignette_luminance=max(dot(c,vec3<f32>(.2627,.6780,.0593)),0.0);
        let highlight_weight=clamp((vignette_luminance-.55)/1.45,0.0,1.0);let protect_weight=1.0-clamp(p.values[3].z,0.0,1.0)*highlight_weight*highlight_weight*(3.0-2.0*highlight_weight);
        c*=exp2(-clamp(p.values[2].z,-1.0,1.0)*2.0*edge_weight*protect_weight);
    }
    output[id.x]=vec4<f32>(finite(c.r),finite(c.g),finite(c.b),source[id.x].a);
}
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
    /// Sum of live wgpu buffer sizes, not physical VRAM allocation or process peak memory.
    pub allocated_buffer_bytes: u64,
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
    #[error(
        "GPU readback exceeded its bounded deadline; preview must use the CPU reference backend"
    )]
    ReadbackTimeout,
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
    pub values: [[f32; 4]; 23],
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
    failure: Arc<AtomicU8>,
    failure_detail: Arc<Mutex<Option<String>>>,
    resources: Mutex<Option<GpuBufferResources>>,
    stats: Mutex<GpuResourceStats>,
}

struct GpuBufferResources {
    capacity_bytes: u64,
    source_fingerprint: u64,
    creative_fingerprint: u64,
    source: wgpu::Buffer,
    output: wgpu::Buffer,
    staging: wgpu::Buffer,
    parameters: wgpu::Buffer,
    /// LUT storage consumed by the fused creative shader. Masks run elsewhere in the graph;
    /// do not reserve full-frame buffers for stages that never bind them.
    curve_lut: wgpu::Buffer,
}

impl GpuBufferResources {
    fn allocated_buffer_bytes(&self) -> u64 {
        [
            &self.source,
            &self.output,
            &self.staging,
            &self.parameters,
            &self.curve_lut,
        ]
        .into_iter()
        .map(wgpu::Buffer::size)
        .sum()
    }
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
        let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
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
        let failure = Arc::new(AtomicU8::new(0));
        let failure_detail = Arc::new(Mutex::new(None));
        let lost = failure.clone();
        let lost_detail = failure_detail.clone();
        device.set_device_lost_callback(move |reason, message| {
            if let Ok(mut detail) = lost_detail.lock() {
                if lost.load(Ordering::Acquire) == 0 {
                    *detail = Some(format!("{reason:?}: {message}"));
                    lost.store(FAILURE_DEVICE_LOST, Ordering::Release);
                }
            } else {
                lost.store(FAILURE_DEVICE_LOST, Ordering::Release);
            }
        });
        let uncaptured = failure.clone();
        let uncaptured_detail = failure_detail.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            let kind = if matches!(error, wgpu::Error::OutOfMemory { .. }) {
                FAILURE_OOM
            } else {
                FAILURE_VALIDATION
            };
            if let Ok(mut detail) = uncaptured_detail.lock() {
                if uncaptured.load(Ordering::Acquire) == 0 {
                    *detail = Some(error.to_string());
                    uncaptured.store(kind, Ordering::Release);
                }
            } else {
                uncaptured.store(kind, Ordering::Release);
            }
        }));
        let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
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
            creative_pipeline,
            exposure_layout,
            creative_layout,
            failure,
            failure_detail,
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

    /// Driver/runtime diagnostics only; resource labels do not contain source photo names/pixels.
    pub fn failure_diagnostic(&self) -> Option<String> {
        self.failure_detail
            .lock()
            .ok()
            .and_then(|detail| detail.clone())
    }

    pub fn resource_stats(&self) -> GpuResourceStats {
        let mut stats = *self.stats.lock().expect("GPU resource statistics");
        stats.allocated_buffer_bytes = self
            .resources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map_or(0, GpuBufferResources::allocated_buffer_bytes);
        stats
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
        self.failure.store(FAILURE_DEVICE_LOST, Ordering::Release);
    }

    /// Explicit test/runtime hook for an allocation failure observed by a scheduler. The caller
    /// must surface the CPU fallback status rather than attempting a hidden retry.
    pub fn mark_out_of_memory(&mut self) {
        self.failure.store(FAILURE_OOM, Ordering::Release);
    }

    fn check_device(&self) -> Result<(), GpuError> {
        match self.failure.load(Ordering::Acquire) {
            0 => Ok(()),
            FAILURE_OOM => Err(GpuError::OutOfMemory),
            FAILURE_DEVICE_LOST => Err(GpuError::DeviceLost),
            _ => Err(GpuError::Validation(
                "uncaptured GPU operation failed".into(),
            )),
        }
    }

    /// Await only this submission, not unrelated work; all paths cancel/unmap the owned staging
    /// mapping. Neither an ignored poll error nor a missing callback may hang a Native worker.
    fn readback_rgba(
        &self,
        buffer: &wgpu::Buffer,
        byte_len: u64,
        submission: wgpu::SubmissionIndex,
    ) -> Result<Vec<[f32; 4]>, GpuError> {
        struct Unmap<'a>(&'a wgpu::Buffer);
        impl Drop for Unmap<'_> {
            fn drop(&mut self) {
                self.0.unmap();
            }
        }
        let slice = buffer.slice(..byte_len);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _unmap = Unmap(buffer);
        let deadline = Instant::now() + GPU_READBACK_LIMIT;
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(GPU_READBACK_LIMIT),
            })
            .map_err(|error| match error {
                wgpu::PollError::Timeout => GpuError::ReadbackTimeout,
                other => GpuError::Readback(other.to_string()),
            })?;
        self.check_device()?;
        wait_for_mapping(&receiver, deadline)?
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        let data = slice
            .get_mapped_range()
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        let result = bytemuck::try_cast_slice::<u8, [f32; 4]>(&data)
            .map_err(|error| GpuError::Readback(error.to_string()))?
            .to_vec();
        drop(data);
        Ok(result)
    }

    /// Creates the canonical RGBA16Float / R16Float resources used by preview and masks. Their
    /// ownership is explicit, so tile/cache eviction can release GPU memory without touching CPU
    /// reference buffers.
    pub fn create_texture_set(&self, width: u32, height: u32) -> Result<GpuTextureSet, GpuError> {
        self.check_device()?;
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
        self.check_device()?;
        if pixels.is_empty()
            || !exposure_ev.is_finite()
            || !pixels.iter().flatten().all(|value| value.is_finite())
        {
            return Err(GpuError::InvalidPixels);
        }
        if pixels.len().div_ceil(64)
            > self.device.limits().max_compute_workgroups_per_dimension as usize
        {
            return Err(GpuError::Unsupported(
                "exposure dispatch exceeds adapter workgroup limit".into(),
            ));
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
            });
        } else {
            self.stats
                .lock()
                .expect("GPU resource statistics")
                .resource_reuse_count += 1;
        }
        let resources = resources.as_mut().expect("allocated GPU resources");
        crate::profiling::record_gpu_buffer_bytes(resources.allocated_buffer_bytes());
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
        let submission = self.queue.submit(Some(encoder.finish()));
        let result = self.readback_rgba(&resources.staging, byte_len, submission)?;
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
        self.check_device()?;
        if pixels.is_empty()
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
        if pixels.len().div_ceil(64)
            > self.device.limits().max_compute_workgroups_per_dimension as usize
        {
            return Err(GpuError::Unsupported(
                "creative dispatch exceeds adapter workgroup limit".into(),
            ));
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
            });
        } else {
            self.stats
                .lock()
                .expect("GPU resource statistics")
                .resource_reuse_count += 1;
        }
        let resources = resources.as_mut().expect("allocated GPU resources");
        crate::profiling::record_gpu_buffer_bytes(resources.allocated_buffer_bytes());
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
        let submission = self.queue.submit(Some(encoder.finish()));
        let readback_started = Instant::now();
        let result = self.readback_rgba(&resources.staging, byte_len, submission)?;
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
    fn mapping_callback_deadline_and_disconnect_are_typed_and_bounded() {
        let (sender, receiver) = std::sync::mpsc::channel::<u8>();
        assert!(matches!(
            wait_for_mapping(&receiver, Instant::now()),
            Err(GpuError::ReadbackTimeout)
        ));
        sender.send(7).unwrap();
        assert_eq!(wait_for_mapping(&receiver, Instant::now()).unwrap(), 7);
        drop(sender);
        assert!(matches!(
            wait_for_mapping(&receiver, Instant::now()),
            Err(GpuError::Readback(_))
        ));
    }

    #[test]
    fn real_device_destroy_callback_reports_loss_before_any_new_gpu_work() {
        let Ok(renderer) = GpuRenderer::try_new() else {
            return;
        };
        renderer.device.destroy();
        let _ = renderer.device.poll(wgpu::PollType::Poll);
        assert!(matches!(renderer.check_device(), Err(GpuError::DeviceLost)));
        assert!(renderer.failure_diagnostic().is_some());
        assert!(matches!(
            renderer.apply_exposure(&[[0.18, 0.18, 0.18, 1.0]], 0.0),
            Err(GpuError::DeviceLost)
        ));
        assert!(matches!(
            renderer.create_texture_set(16, 16),
            Err(GpuError::DeviceLost)
        ));
        let status = resolve_preview_backend(true, Err(GpuError::DeviceLost));
        assert_eq!(status.backend, GpuBackendKind::CpuFallback);
        assert!(status.reason.is_some());
    }

    #[test]
    fn explicit_oom_is_not_mislabeled_invalid_pixels_or_device_loss() {
        let Ok(mut renderer) = GpuRenderer::try_new() else {
            return;
        };
        renderer.mark_out_of_memory();
        assert!(matches!(
            renderer.apply_exposure(&[[0.18, 0.18, 0.18, 1.0]], 0.0),
            Err(GpuError::OutOfMemory)
        ));
        assert!(matches!(
            renderer.apply_creative(
                &[[0.18, 0.18, 0.18, 1.0]],
                &GpuCreativeParameters {
                    values: [[0.0; 4]; 23]
                },
                &[0.0; 8192],
                None
            ),
            Err(GpuError::OutOfMemory)
        ));
    }

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
            values: [[0.0; 4]; 23],
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
        assert_eq!(stats.allocated_buffer_bytes, 3 * 128 * 16 + 512 + 32768);
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
        assert_eq!(renderer.resource_stats().allocated_buffer_bytes, 0);
        let small = vec![[0.1, 0.2, 0.3, 1.0]; 64];
        let large = vec![[0.1, 0.2, 0.3, 1.0]; 256];
        let (result, profile) = crate::profiling::capture(|| renderer.apply_exposure(&small, 0.0));
        assert_eq!(result.expect("small frame"), small);
        assert_eq!(profile.gpu_buffer_bytes, 3 * 64 * 16 + 33280);
        assert_eq!(
            renderer.resource_stats().allocated_buffer_bytes,
            3 * 64 * 16 + 33280
        );
        renderer.apply_exposure(&large, 0.0).expect("resized frame");
        assert_eq!(renderer.resource_stats().source_upload_count, 2);
        assert_eq!(
            renderer.resource_stats().allocated_buffer_bytes,
            3 * 256 * 16 + 33280
        );
        renderer
            .apply_exposure(&small, 0.0)
            .expect("reuse larger capacity");
        assert_eq!(
            renderer.resource_stats().allocated_buffer_bytes,
            3 * 256 * 16 + 33280
        );
        renderer.reset_resource_stats();
        assert_eq!(
            renderer.resource_stats().allocated_buffer_bytes,
            3 * 256 * 16 + 33280
        );
    }
}
