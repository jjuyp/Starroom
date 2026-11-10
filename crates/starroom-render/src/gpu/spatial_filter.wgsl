// Existing separable Gaussian coefficients, and compensated separable box mean.
// Box is mathematically the CPU summed-area mean, with explicitly tested GPU f32 rounding.
struct Parameters { shape: vec4<u32>, mode: vec4<u32>, };
@group(0) @binding(0) var<storage, read> source: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> weights: array<f32>;
@group(0) @binding(3) var<uniform> p: Parameters;
@compute @workgroup_size(64)
fn filter_main(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= p.shape.z) { return; }
  let width = i32(p.shape.x); let height = i32(p.shape.y);
  let reach = i32(p.shape.w);
  let pixel = id.x;
  let x = i32(pixel % p.shape.x); let y = i32(pixel / p.shape.x);
  var sum = vec3<f32>(0.0); var compensation = vec3<f32>(0.0); var count = 0u;
  for (var offset = -reach; offset <= reach; offset++) {
    var sx = x; var sy = y;
    if (p.mode.x == 0u) { sx += offset; } else { sy += offset; }
    if (p.mode.w == 0u) {
      sum += source[u32(clamp(sy, 0, height - 1) * width + clamp(sx, 0, width - 1))].rgb * weights[p.mode.y + u32(offset + reach)];
    } else if (sx >= 0 && sx < width && sy >= 0 && sy < height) {
      let value = source[u32(sy * width + sx)].rgb - compensation;
      let next = sum + value;
      compensation = (next - sum) - value;
      sum = next; count++;
    }
  }
  if (p.mode.w == 2u) {
    let c = sum / f32(max(count, 1u));
    // Keep fine RGB, store the broad mean's Y in its otherwise unused alpha slot.
    output[pixel].a = 0.2627 * c.r + 0.6780 * c.g + 0.0593 * c.b;
  } else {
    if (p.mode.w == 1u) { sum /= f32(max(count, 1u)); }
    output[p.mode.z + pixel] = vec4<f32>(sum, source[pixel].a);
  }
}
