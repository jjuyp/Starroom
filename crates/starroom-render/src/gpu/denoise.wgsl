// GPU port of starroom-detail's deterministic edge-aware Y/Cb/Cr kernel.
// Preserve neighbour/accumulation order and unbounded linear RGB. No display math.
struct Parameters {
  shape: vec4<u32>,
  strengths: vec4<f32>,
};
@group(0) @binding(0) var<storage, read> input_pixels: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output_pixels: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: Parameters;
@group(0) @binding(3) var<storage, read> weights: array<f32>;

fn components(c: vec3<f32>) -> vec3<f32> {
  let y = 0.2627 * c.r + 0.6780 * c.g + 0.0593 * c.b;
  return vec3<f32>(y, c.b - y, c.r - y);
}

@compute @workgroup_size(64)
fn denoise_main(@builtin(global_invocation_id) id: vec3<u32>) {
  let index = id.x;
  if (index >= p.shape.z) { return; }
  let original = input_pixels[index];
  let c = components(original.rgb);
  let width = i32(p.shape.x);
  let height = i32(p.shape.y);
  let x = i32(index % p.shape.x);
  let y = i32(index / p.shape.x);
  let reach = i32(p.shape.w);
  var sum = vec3<f32>(0.0);
  var weight_sum = 0.0;
  var k = 0u;
  for (var dy = -reach; dy <= reach; dy++) {
    for (var dx = -reach; dx <= reach; dx++) {
      let sx = clamp(x + dx, 0, width - 1);
      let sy = clamp(y + dy, 0, height - 1);
      let sample = components(input_pixels[u32(sy * width + sx)].rgb);
      let difference = (sample.x - c.x) / p.strengths.z;
      let range = exp(-0.5 * (difference * difference));
      let weight = weights[k] * range;
      sum += sample * weight;
      weight_sum += weight;
      k++;
    }
  }
  let filtered = sum / max(weight_sum, 1.1920929e-7);
  let out_y = c.x + (filtered.x - c.x) * p.strengths.x * (1.0 - p.strengths.w * 0.7);
  let cb = c.y + (filtered.y - c.y) * p.strengths.y;
  let cr = c.z + (filtered.z - c.z) * p.strengths.y;
  let r = out_y + cr;
  let b = out_y + cb;
  let g = (out_y - 0.2627 * r - 0.0593 * b) / 0.6780;
  output_pixels[index] = vec4<f32>(r, g, b, original.a);
}
