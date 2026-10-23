//! Real kernel diagnostic, NOT UI latency, RSS, GPU or full-image acceptance.
use starroom_detail::{
    DenoiseParameters, DetailCancelled, LinearImage, denoise_owned, denoise_owned_cancellable,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn quantile(samples: &[f64], percentile: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * percentile).ceil() as usize]
}

fn main() {
    let (width, height) = (1024, 683);
    let source = LinearImage::new(
        width,
        height,
        (0..width * height * 3)
            .map(|index| (index * 97 % 1021) as f32 / 317.0 - 0.15)
            .collect(),
    )
    .unwrap();
    let parameters = DenoiseParameters {
        luminance: 0.7,
        chroma: 0.8,
        radius: 4.0,
        ..Default::default()
    };
    let token = AtomicBool::new(false);
    let expected = denoise_owned(source.clone(), parameters);
    let mut baseline = Vec::new();
    let mut controlled = Vec::new();
    for _ in 0..5 {
        let input = source.clone();
        let start = Instant::now();
        let old = denoise_owned(input, parameters);
        baseline.push(start.elapsed().as_secs_f64() * 1000.0);
        let input = source.clone();
        let start = Instant::now();
        let new = denoise_owned_cancellable(input, parameters, &token).unwrap();
        controlled.push(start.elapsed().as_secs_f64() * 1000.0);
        assert!(
            new.data
                .iter()
                .zip(&old.data)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
        assert!(
            new.data
                .iter()
                .zip(&expected.data)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
    let mut settle = Vec::new();
    for _ in 0..5 {
        let input = source.clone();
        let cancelled = AtomicBool::new(false);
        let (started_tx, started_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                started_tx.send(()).unwrap();
                denoise_owned_cancellable(input, parameters, &cancelled)
            });
            started_rx.recv().unwrap();
            std::thread::sleep(Duration::from_millis(2));
            let requested = Instant::now();
            cancelled.store(true, Ordering::Release);
            assert_eq!(worker.join().unwrap(), Err(DetailCancelled));
            settle.push(requested.elapsed().as_secs_f64() * 1000.0);
        });
    }
    println!(
        "{{\"width\":{width},\"height\":{height},\"changedSamples\":5,\"bitExact\":true,\"baselineMs\":{baseline:?},\"controlledMs\":{controlled:?},\"cancelSettleMs\":{settle:?},\"baselineMedianMs\":{},\"controlledMedianMs\":{},\"cancelSettleMedianMs\":{},\"cancelSettleP95Ms\":{}}}",
        quantile(&baseline, 0.5),
        quantile(&controlled, 0.5),
        quantile(&settle, 0.5),
        quantile(&settle, 0.95)
    );
}
