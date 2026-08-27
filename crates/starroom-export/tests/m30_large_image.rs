use image::{ExtendedColorType, ImageEncoder, ImageReader, codecs::jpeg::JpegEncoder};
use starroom_export::{
    ExportFormat, ExportRequest, ExportSettings, MetadataPolicy, NativeSharedGraphRenderer,
    export_one_profiled,
};
use starroom_heal::{HealMode, HealPoint, HealingOperation, SourceMode};
use starroom_imageio::decode_source_preview;
use starroom_pipeline::{
    LayerAdjustments, LayerBlendMode, NativeAdjustmentLayer, RenderSettings,
    render_source_preview_to_srgb8,
};
use starroom_project::MaskDefinition;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

fn root() -> PathBuf {
    let value =
        std::env::temp_dir().join(format!("starroom-m30-large-image-{}", std::process::id()));
    let _ = fs::remove_dir_all(&value);
    fs::create_dir_all(&value).expect("create M30 large-image root");
    value
}

fn write_gradient_jpeg(path: &Path, width: u32, height: u32) {
    let mut rgb = vec![0_u8; width as usize * height as usize * 3];
    for (index, pixel) in rgb.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let x = index % width as usize;
        let y = index / width as usize;
        pixel[0] = ((x * 255) / width as usize) as u8;
        pixel[1] = ((y * 255) / height as usize) as u8;
        pixel[2] = (((x + y) * 255) / (width as usize + height as usize)) as u8;
    }
    let encoder = JpegEncoder::new_with_quality(File::create(path).unwrap(), 90);
    encoder
        .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        .expect("encode large source fixture");
}

fn settings() -> RenderSettings {
    let mut tone = RenderSettings::default().tone;
    tone.exposure_ev = 0.25;
    tone.highlights = -0.2;
    tone.shadows = 0.2;

    let mut layer_adjustments = LayerAdjustments::default();
    layer_adjustments.tone.exposure_ev = 0.2;
    let layers = vec![NativeAdjustmentLayer {
        id: "m30-radial".into(),
        name: "M30 radial mask".into(),
        enabled: true,
        opacity: 0.6,
        blend_mode: LayerBlendMode::Normal,
        mask: MaskDefinition::Radial {
            x: 0.5,
            y: 0.5,
            width: 0.4,
            height: 0.5,
            rotation: 18.0,
            feather: 0.35,
            invert: false,
        }
        .into(),
        adjustments: layer_adjustments,
    }];
    let healing_operations = vec![HealingOperation {
        id: "m30-heal".into(),
        enabled: true,
        mode: HealMode::Heal,
        target: HealPoint { x: 0.55, y: 0.52 },
        source: Some(HealPoint { x: 0.45, y: 0.52 }),
        radius: 16.0,
        feather: 0.5,
        opacity: 0.8,
        rotation_degrees: 0.0,
        scale: 1.0,
        tone_adaptation: true,
        texture_adaptation: true,
        source_mode: SourceMode::Manual,
        metadata: BTreeMap::new(),
    }];
    RenderSettings {
        tone,
        layers,
        healing_operations,
        image_identity: "m30-large-gradient".into(),
        ..RenderSettings::default()
    }
}

#[test]
#[ignore = "explicit M30 high-memory release gate"]
fn m30_real_24_45_60_100mp_open_preview_mask_heal_and_export() {
    assert_eq!(
        std::env::var("STARROOM_M30_LARGE_IMAGE").as_deref(),
        Ok("true"),
        "large-image gate must be explicitly enabled"
    );
    let root = root();
    for (label, width, height) in [
        ("24mp", 6000, 4000),
        ("45mp", 8192, 5492),
        ("60mp", 9500, 6316),
        ("100mp", 12_250, 8164),
    ] {
        let source = root.join(format!("{label}.jpg"));
        let generate_started = Instant::now();
        write_gradient_jpeg(&source, width, height);
        let generate_time = generate_started.elapsed();
        let source_bytes = fs::read(&source).unwrap();

        let preview_started = Instant::now();
        let preview_source =
            decode_source_preview(&source, 1024).expect("large-image preview open");
        let preview = render_source_preview_to_srgb8(&preview_source, &settings())
            .expect("large-image masked/healed preview");
        let preview_time = preview_started.elapsed();
        assert!(preview.width <= 1024 && preview.height <= 1024);

        let request = ExportRequest {
            asset_id: 1,
            source_path: source.clone(),
            destination_directory: root.join("exports"),
            original_name: format!("{label}.jpg"),
            capture_date: None,
            rating: 0,
            keywords: vec!["m30-scale".into()],
            camera: None,
            look: None,
            sequence: 1,
            source_fingerprint: format!("m30-{label}"),
            edit_state_identity: "m30-large-image-v1".into(),
            settings: ExportSettings {
                format: ExportFormat::Jpeg,
                metadata: MetadataPolicy::None,
                filename_template: format!("{label}-rendered"),
                ..ExportSettings::default()
            },
        };
        let export_started = Instant::now();
        let (result, profile) = export_one_profiled(
            &NativeSharedGraphRenderer,
            &request,
            &settings(),
            &AtomicBool::new(false),
        );
        let export_time = export_started.elapsed();
        let destination = result
            .expect("large-image production export")
            .destination
            .unwrap();
        let dimensions = ImageReader::open(&destination)
            .unwrap()
            .into_dimensions()
            .unwrap();
        assert_eq!(dimensions, (width, height));
        assert_eq!(fs::read(&source).unwrap(), source_bytes);
        eprintln!(
            "M30_LARGE_IMAGE label={label} pixels={} generate_ms={:.2} preview_ms={:.2} export_ms={:.2} process_peak_bytes={}",
            u64::from(width) * u64::from(height),
            generate_time.as_secs_f64() * 1000.0,
            preview_time.as_secs_f64() * 1000.0,
            export_time.as_secs_f64() * 1000.0,
            profile.process_peak_working_set_bytes.unwrap_or(0),
        );
        fs::remove_file(destination).unwrap();
        fs::remove_file(source).unwrap();
    }
}
