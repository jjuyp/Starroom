// Pointwise copies of the accepted local-detail/sharpen CPU semantics.
struct Parameters {
  shape: vec4<u32>, local: vec4<f32>, sharpen: vec4<f32>, extra: vec4<f32>,
};
@group(0) @binding(0) var<storage, read_write> pixels: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> filtered: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: Parameters;
fn luminance(c: vec3<f32>) -> f32 { return 0.2627 * c.r + 0.6780 * c.g + 0.0593 * c.b; }
fn sstep(a:f32, b:f32, v:f32) -> f32 {
  let t = clamp((v-a)/max(b-a,1e-7),0.0,1.0); return t*t*(3.0-2.0*t);
}
@compute @workgroup_size(64)
fn merge_main(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= p.shape.z) { return; }
  let original = pixels[id.x]; let y = luminance(original.rgb);
  let fine = luminance(filtered[id.x].rgb);
  let medium = luminance(filtered[p.shape.z + id.x].rgb);
  var scale = 1.0;
  if (p.shape.w == 0u) {
    let broad = filtered[id.x].a;
    let adjusted_y = y + (y-fine)*p.local.x*0.8 + (fine-medium)*p.local.y*1.15
      + (y-broad)*p.local.z*0.7 + p.local.z*(0.18-broad)*0.10;
    if (abs(y) > 1e-7) { scale = max(adjusted_y,0.0)/y; }
  } else {
    let fine_detail = y-fine; let coarse_detail = fine-medium;
    let detail = fine_detail*(0.55+p.sharpen.y*0.9) + coarse_detail*(1.0-p.sharpen.y)*0.35;
    var mask = 1.0;
    if (p.sharpen.z > 1.1920929e-7) { mask = sstep(p.extra.x,p.extra.x+0.03+p.sharpen.z*0.08,abs(y-medium)); }
    let local_range = abs(fine-medium) + abs(fine_detail) + 0.002;
    let limit = local_range*(2.2-p.sharpen.w*1.6);
    let delta = clamp(detail*p.sharpen.x*mask,-limit,limit);
    if (abs(y) > 1e-7) { scale = (y+delta)/y; }
  }
  pixels[id.x] = vec4<f32>(original.rgb*scale, original.a);
}
