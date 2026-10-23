//! Allocation ownership/packing probe only, not an image/whole-process memory benchmark.
use std::{hint::black_box, time::Instant};
type PackingMethod = fn(&[[f32; 4]]) -> Vec<f32>;

fn old_encoded(rgba: &[[f32; 4]]) -> Vec<f32> {
    let triples: Vec<[f32; 3]> = rgba.iter().map(|p| [p[0], p[1], p[2]]).collect();
    triples.into_iter().flatten().collect()
}
fn flat_extend(rgba: &[[f32; 4]]) -> Vec<f32> {
    let mut data = Vec::new();
    data.try_reserve_exact(rgba.len() * 3)
        .expect("bounded probe allocation");
    for p in rgba {
        data.extend_from_slice(&p[..3]);
    }
    data
}
fn flat_assign(rgba: &[[f32; 4]]) -> Vec<f32> {
    let mut data = Vec::new();
    data.try_reserve_exact(rgba.len() * 3)
        .expect("bounded probe allocation");
    data.resize(rgba.len() * 3, 0.0);
    for (output, p) in data.as_chunks_mut::<3>().0.iter_mut().zip(rgba) {
        *output = [p[0], p[1], p[2]];
    }
    data
}
fn main() {
    let original = vec![0.25_f32; 1024 * 680 * 3];
    let triples: Vec<[f32; 3]> = original
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    let triple_pointer = triples.as_ptr() as usize;
    let flat: Vec<f32> = triples.into_iter().flatten().collect();
    assert_eq!(flat, original);
    let rgba: Vec<_> = (0..1024 * 680)
        .map(|i| [i as f32 / 65535.0, -0.0, 0.25, 1.0])
        .collect();
    let expected = old_encoded(&rgba);
    let methods: [PackingMethod; 3] = [old_encoded, flat_extend, flat_assign];
    let mut times = [Vec::new(), Vec::new(), Vec::new()];
    for round in 0..8 {
        // Rotate order to avoid always giving the last method the warmest CPU/cache state.
        for offset in 0..3 {
            let index = (round + offset) % 3;
            let start = Instant::now();
            let data = black_box(methods[index](black_box(&rgba)));
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            assert!(
                data.iter()
                    .zip(&expected)
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            );
            if round > 0 {
                times[index].push(elapsed);
            }
        }
    }
    for samples in &mut times {
        samples.sort_by(f64::total_cmp);
    }
    println!(
        "{}",
        serde_json::json!({
            "samples": flat.len(), "floatBufferBytes": flat.capacity()*4,
            "flattenReusesAllocation": flat.as_ptr() as usize == triple_pointer,
            "scope": "owned1024x680 RGBA-to-RGB packing only; no ICC/graph/UI/100MP claim",
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "packingMedianMs": { "previous": times[0][3], "singleExtend": times[1][3], "singleAssign": times[2][3] },
        })
    );
}
