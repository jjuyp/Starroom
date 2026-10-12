//! Allocation ownership probe only, not an image/whole-process memory benchmark.
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
    println!(
        "{}",
        serde_json::json!({
            "samples": flat.len(), "floatBufferBytes": flat.capacity()*4,
            "flattenReusesAllocation": flat.as_ptr() as usize == triple_pointer,
        })
    );
}
