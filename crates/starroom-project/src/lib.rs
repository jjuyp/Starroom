//! Serializable, storage-independent project state.

use serde::{Deserialize, Serialize};
use starroom_core::{GlobalAdjustments, SourceIdentity};
use std::{collections::BTreeMap, fs, io::Write, path::Path};
use thiserror::Error;

pub const PROJECT_SCHEMA_VERSION: u32 = 2;
pub const PROJECT_MINIMUM_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("project file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("project layer stack is invalid: {0}")]
    InvalidLayers(&'static str),
    #[error("project schema version is unsupported: {0}")]
    UnsupportedSchema(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema_version: u32,
    pub engine_version: String,
    pub source: SourceIdentity,
    pub global_adjustments: GlobalAdjustments,
    #[serde(default)]
    pub camera_profile: Option<PersistedCameraProfile>,
    /// M5 white-balance intent is stored independently from slider values so RAW Camera/As-Shot
    /// and encoded Relative semantics cannot change silently when a project is reopened.
    #[serde(default)]
    pub white_balance: PersistedWhiteBalance,
    #[serde(default)]
    pub tone_curves: PersistedToneCurves,
    #[serde(default)]
    pub color_mixer: PersistedColorMixer,
    #[serde(default)]
    pub color_grading: PersistedColorGrading,
    #[serde(default)]
    pub detail: PersistedDetail,
    #[serde(default)]
    pub optics: PersistedOptics,
    #[serde(default)]
    pub geometry: PersistedGeometry,
    #[serde(default)]
    pub skin_retouch: PersistedSkinRetouch,
    #[serde(default)]
    pub healing_operations: Vec<PersistedHealingOperation>,
    #[serde(default)]
    pub masks: Vec<MaskNode>,
    #[serde(default)]
    pub layers: Vec<AdjustmentLayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedCameraProfile {
    pub id: String,
    pub version: String,
    pub hash: String,
    /// `resolved` or `generic`; stored explicitly so reopening never silently changes policy.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWhiteBalance {
    pub mode: String,
    pub temperature: f32,
    pub tint: f32,
    pub sample: Option<PersistedWhiteBalanceSample>,
}

impl Default for PersistedWhiteBalance {
    fn default() -> Self {
        Self {
            mode: "sourceDefault".into(),
            temperature: 0.0,
            tint: 0.0,
            sample: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWhiteBalanceSample {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedToneCurves {
    pub master: Vec<PersistedCurvePoint>,
    pub red: Vec<PersistedCurvePoint>,
    pub green: Vec<PersistedCurvePoint>,
    pub blue: Vec<PersistedCurvePoint>,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PersistedCurvePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedColorBand {
    pub hue: f32,
    pub chroma: f32,
    pub lightness: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedColorMixer {
    pub bands: [PersistedColorBand; 8],
    pub hue_lock: bool,
    pub band_width_degrees: f32,
}

impl Default for PersistedColorMixer {
    fn default() -> Self {
        Self {
            bands: [PersistedColorBand::default(); 8],
            hue_lock: true,
            band_width_degrees: 52.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedColorWheel {
    pub hue_degrees: f32,
    pub chroma: f32,
    pub lightness: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedColorGrading {
    pub shadows: PersistedColorWheel,
    pub midtones: PersistedColorWheel,
    pub highlights: PersistedColorWheel,
    pub global: PersistedColorWheel,
    pub balance: f32,
    pub blending: f32,
    pub amount: f32,
}

impl Default for PersistedColorGrading {
    fn default() -> Self {
        Self {
            shadows: PersistedColorWheel::default(),
            midtones: PersistedColorWheel::default(),
            highlights: PersistedColorWheel::default(),
            global: PersistedColorWheel::default(),
            balance: 0.0,
            blending: 0.5,
            amount: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedDetail {
    pub sharpen_amount: f32,
    pub sharpen_radius: f32,
    pub sharpen_detail: f32,
    pub sharpen_masking: f32,
    pub halo_protection: f32,
    pub denoise_luminance: f32,
    pub denoise_chroma: f32,
    pub denoise_radius: f32,
    pub detail_protection: f32,
    pub high_iso: f32,
    pub texture: f32,
    pub clarity: f32,
    pub dehaze: f32,
}

impl Default for PersistedDetail {
    fn default() -> Self {
        Self {
            sharpen_amount: 0.0,
            sharpen_radius: 1.0,
            sharpen_detail: 0.5,
            sharpen_masking: 0.0,
            halo_protection: 0.75,
            denoise_luminance: 0.0,
            denoise_chroma: 0.0,
            denoise_radius: 1.25,
            detail_protection: 0.5,
            high_iso: 0.0,
            texture: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedOptics {
    pub enabled: bool,
    pub distortion: bool,
    pub tca: bool,
    pub vignette: bool,
    pub auto_scale: bool,
    pub match_mode: String,
    pub profile_id: Option<String>,
    pub profile_status: Option<String>,
    pub database_version: String,
    pub manual_identity: Option<PersistedLensIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedLensIdentity {
    pub camera_make: String,
    pub camera_model: String,
    pub lens_make: String,
    pub lens_model: String,
    pub focal_length_mm: f32,
    pub aperture: f32,
    pub focus_distance_m: Option<f32>,
}

impl Default for PersistedOptics {
    fn default() -> Self {
        Self {
            enabled: false,
            distortion: true,
            tca: true,
            vignette: true,
            auto_scale: true,
            match_mode: "auto".into(),
            profile_id: None,
            profile_status: None,
            database_version: "0.3.4".into(),
            manual_identity: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedGeometry {
    pub rotation_degrees: f32,
    pub vertical_keystone: f32,
    pub horizontal_keystone: f32,
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub crop_left: f32,
    pub crop_top: f32,
    pub crop_right: f32,
    pub crop_bottom: f32,
    pub crop_aspect_width: f32,
    pub crop_aspect_height: f32,
    pub four_point: Option<[PersistedPoint; 4]>,
    pub upright_mode: String,
}

/// M17 metadata-only sidecar state. Semantic rasters remain native cache data, never JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSkinRetouch {
    pub smooth: f32,
    pub texture: f32,
    pub tone_evenness: f32,
    pub hue_degrees: f32,
    pub chroma: f32,
    pub exposure_ev: f32,
    #[serde(default)]
    pub faces: Vec<PersistedPortraitFace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedPortraitFace {
    pub face_id: String,
    pub cache_key: String,
}

/// M18 operation persistence contains source-coordinate intent only. `aiInpaint` remains a
/// reserved serialized mode and is not considered an implemented provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedHealingOperation {
    pub id: String,
    pub enabled: bool,
    pub mode: String,
    pub target: PersistedPoint,
    pub source: Option<PersistedPoint>,
    pub radius: f32,
    pub feather: f32,
    pub opacity: f32,
    pub rotation_degrees: f32,
    pub scale: f32,
    pub tone_adaptation: bool,
    pub texture_adaptation: bool,
    pub source_mode: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl Default for PersistedGeometry {
    fn default() -> Self {
        Self {
            rotation_degrees: 0.0,
            vertical_keystone: 0.0,
            horizontal_keystone: 0.0,
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            crop_left: 0.0,
            crop_top: 0.0,
            crop_right: 1.0,
            crop_bottom: 1.0,
            crop_aspect_width: 0.0,
            crop_aspect_height: 0.0,
            four_point: None,
            upright_mode: "off".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskNode {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BlendMode {
    #[default]
    Normal,
    Luminosity,
    Color,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MaskDefinition {
    None,
    Radial {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rotation: f32,
        feather: f32,
        invert: bool,
    },
    Linear {
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        feather: f32,
        #[serde(default)]
        invert: bool,
    },
    Brush {
        points: Vec<BrushPoint>,
        radius: f32,
        feather: f32,
        flow: f32,
        #[serde(default)]
        erase: bool,
    },
    Luminance {
        minimum: f32,
        maximum: f32,
        feather: f32,
        #[serde(default)]
        invert: bool,
    },
    ColorRange {
        reference: [f32; 3],
        tolerance: f32,
        feather: f32,
        #[serde(default)]
        invert: bool,
    },
    /// M16 semantic mask generated by the local portrait provider. It is a normal MaskTree leaf
    /// and therefore composes with Add/Subtract/Intersect/Invert rather than creating a second
    /// portrait-only compositing path. Raster values stay in the native cache, never in JSON.
    PortraitSemantic {
        face_id: String,
        region: PortraitMaskRegion,
        threshold: f32,
        feather: f32,
        model_id: String,
        model_version: String,
        model_hash: String,
        cache_key: String,
    },
    /// M20 local AI result. The node stores only reproducible provider/model identity and
    /// refinement intent; the R16Float-compatible raster remains in the native cache.
    Generated {
        provider_id: String,
        model_id: String,
        model_version: String,
        model_hash: String,
        semantic_class: GeneratedMaskSemantic,
        threshold: f32,
        feather: f32,
        #[serde(default)]
        invert: bool,
        cache_identity: String,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    Provider {
        provider: String,
        request: String,
        fingerprint: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum GeneratedMaskSemantic {
    Subject,
    Background,
    Person,
    Sky,
    Skin,
    Hair,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum PortraitMaskRegion {
    Face,
    Skin,
    Eyes,
    LeftEye,
    RightEye,
    Brows,
    LeftBrow,
    RightBrow,
    Lips,
    Mouth,
    Hair,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MaskOperation {
    Add,
    Subtract,
    Intersect,
    Invert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MaskComposite {
    pub operation: MaskOperation,
    pub children: Vec<MaskTree>,
}

/// Serializable non-destructive mask expression. `untagged` keeps the original v0.2 leaf-mask
/// JSON readable while allowing Add/Subtract/Intersect compositions without rasterizing them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MaskTree {
    Leaf(MaskDefinition),
    Composite(MaskComposite),
}

impl From<MaskDefinition> for MaskTree {
    fn from(value: MaskDefinition) -> Self {
        Self::Leaf(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrushPoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentLayer {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub opacity: f32,
    #[serde(default)]
    pub blend_mode: BlendMode,
    pub order: u32,
    pub mask: MaskTree,
    /// Storage-independent parameter map. Typed engine parameters are resolved by schema/version.
    #[serde(default)]
    pub adjustments: BTreeMap<String, f32>,
}

impl Project {
    pub fn validate_schema(&self) -> Result<(), ProjectError> {
        if !(PROJECT_MINIMUM_SCHEMA_VERSION..=PROJECT_SCHEMA_VERSION).contains(&self.schema_version)
        {
            return Err(ProjectError::UnsupportedSchema(self.schema_version));
        }
        Ok(())
    }

    /// Validates the persisted non-destructive layer document. Rendering has a separate typed
    /// request validation, but sidecars must never persist ambiguous order or invalid opacity.
    pub fn validate_layers(&self) -> Result<(), ProjectError> {
        let mut ids = std::collections::BTreeSet::new();
        let mut orders = std::collections::BTreeSet::new();
        for layer in &self.layers {
            if layer.id.trim().is_empty() || !ids.insert(&layer.id) {
                return Err(ProjectError::InvalidLayers(
                    "ids must be non-empty and unique",
                ));
            }
            if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
                return Err(ProjectError::InvalidLayers(
                    "opacity must be finite and 0..1",
                ));
            }
            if !orders.insert(layer.order) {
                return Err(ProjectError::InvalidLayers("order values must be unique"));
            }
            if layer.adjustments.values().any(|value| !value.is_finite()) {
                return Err(ProjectError::InvalidLayers("adjustments must be finite"));
            }
        }
        Ok(())
    }

    pub fn write_sidecar(&self, path: impl AsRef<Path>) -> Result<(), ProjectError> {
        self.validate_schema()?;
        self.validate_layers()?;
        let json = serde_json::to_vec_pretty(self)?;
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&json)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| ProjectError::Io(error.error))?;
        Ok(())
    }

    pub fn read_sidecar(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let bytes = fs::read(path)?;
        let project: Self = serde_json::from_slice(&bytes)?;
        project.validate_schema()?;
        project.validate_layers()?;
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjustment_state_and_layers_round_trip() {
        let mut adjustments = BTreeMap::new();
        adjustments.insert("exposure".into(), 0.35);
        let project = Project {
            schema_version: 2,
            engine_version: "0.2.0".into(),
            source: SourceIdentity {
                path: "photo.jpg".into(),
                content_hash: "abc".into(),
                byte_length: 42,
            },
            global_adjustments: GlobalAdjustments {
                exposure_ev: 0.75,
                ..Default::default()
            },
            camera_profile: Some(PersistedCameraProfile {
                id: "dng-forward-matrix:test:camera".into(),
                version: "starroom-camera-profile-v1".into(),
                hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                status: "resolved".into(),
            }),
            white_balance: PersistedWhiteBalance {
                mode: "asShot".into(),
                temperature: 0.0,
                tint: 0.0,
                sample: Some(PersistedWhiteBalanceSample {
                    x: 0.4,
                    y: 0.4,
                    width: 0.1,
                    height: 0.1,
                }),
            },
            tone_curves: PersistedToneCurves {
                master: vec![
                    PersistedCurvePoint { x: 0.0, y: 0.0 },
                    PersistedCurvePoint { x: 1.0, y: 1.0 },
                ],
                red: vec![],
                green: vec![],
                blue: vec![],
                preset: Some("identity".into()),
            },
            color_mixer: PersistedColorMixer {
                bands: [PersistedColorBand::default(); 8],
                hue_lock: true,
                band_width_degrees: 52.0,
            },
            color_grading: PersistedColorGrading::default(),
            detail: PersistedDetail::default(),
            optics: PersistedOptics::default(),
            geometry: PersistedGeometry {
                rotation_degrees: 2.5,
                crop_aspect_width: 3.0,
                crop_aspect_height: 2.0,
                upright_mode: "level".into(),
                ..Default::default()
            },
            skin_retouch: PersistedSkinRetouch {
                smooth: 0.35,
                texture: 0.7,
                tone_evenness: 0.2,
                hue_degrees: 4.0,
                chroma: -0.1,
                exposure_ev: 0.15,
                faces: vec![PersistedPortraitFace {
                    face_id: "face-a".into(),
                    cache_key: "cache-a".into(),
                }],
            },
            healing_operations: vec![PersistedHealingOperation {
                id: "heal-a".into(),
                enabled: true,
                mode: "heal".into(),
                target: PersistedPoint { x: 0.5, y: 0.5 },
                source: None,
                radius: 24.0,
                feather: 0.5,
                opacity: 0.8,
                rotation_degrees: 0.0,
                scale: 1.0,
                tone_adaptation: true,
                texture_adaptation: true,
                source_mode: "auto".into(),
                metadata: BTreeMap::new(),
            }],
            masks: vec![],
            layers: vec![AdjustmentLayer {
                id: "portrait-light".into(),
                name: "Portrait Light".into(),
                enabled: true,
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                order: 0,
                mask: MaskDefinition::Provider {
                    provider: "subject".into(),
                    request: "person".into(),
                    fingerprint: None,
                }
                .into(),
                adjustments,
            }],
        };
        let json = serde_json::to_string(&project).expect("serialize");
        let restored: Project = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.global_adjustments.exposure_ev, 0.75);
        assert_eq!(restored.source.content_hash, "abc");
        assert!(restored.color_mixer.hue_lock);
        assert_eq!(restored.color_mixer.bands.len(), 8);
        assert_eq!(restored.color_grading.amount, 1.0);
        assert_eq!(restored.detail.sharpen_radius, 1.0);
        assert_eq!(restored.optics.database_version, "0.3.4");
        assert_eq!(restored.geometry.rotation_degrees, 2.5);
        assert_eq!(restored.geometry.upright_mode, "level");
        assert_eq!(
            restored.camera_profile.as_ref().unwrap().hash,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(restored.white_balance.mode, "asShot");
        assert_eq!(restored.tone_curves.preset.as_deref(), Some("identity"));
        assert_eq!(restored.skin_retouch.faces[0].cache_key, "cache-a");
        assert_eq!(restored.healing_operations[0].mode, "heal");
        assert_eq!(restored.layers.len(), 1);
        assert_eq!(restored.layers[0].adjustments.get("exposure"), Some(&0.35));
        assert!(restored.validate_layers().is_ok());
    }

    #[test]
    fn mask_tree_round_trips_add_subtract_and_intersect() {
        let tree = MaskTree::Composite(MaskComposite {
            operation: MaskOperation::Intersect,
            children: vec![
                MaskDefinition::Provider {
                    provider: "subject".into(),
                    request: "person".into(),
                    fingerprint: Some("model-v1".into()),
                }
                .into(),
                MaskTree::Composite(MaskComposite {
                    operation: MaskOperation::Subtract,
                    children: vec![
                        MaskDefinition::Radial {
                            x: 0.5,
                            y: 0.5,
                            width: 0.8,
                            height: 0.8,
                            rotation: 0.0,
                            feather: 0.25,
                            invert: false,
                        }
                        .into(),
                        MaskDefinition::Brush {
                            points: vec![BrushPoint {
                                x: 0.45,
                                y: 0.42,
                                pressure: 1.0,
                            }],
                            radius: 0.04,
                            feather: 0.5,
                            flow: 1.0,
                            erase: false,
                        }
                        .into(),
                    ],
                }),
            ],
        });
        let json = serde_json::to_string(&tree).expect("serialize mask tree");
        let restored: MaskTree = serde_json::from_str(&json).expect("deserialize mask tree");
        assert_eq!(restored, tree);
    }

    #[test]
    fn legacy_leaf_mask_json_remains_readable_as_tree() {
        let json = r#"{"type":"radial","x":0.5,"y":0.5,"width":0.4,"height":0.4,"rotation":0.0,"feather":0.5,"invert":false}"#;
        let restored: MaskTree = serde_json::from_str(json).expect("deserialize legacy leaf");
        assert!(matches!(
            restored,
            MaskTree::Leaf(MaskDefinition::Radial { .. })
        ));
    }

    #[test]
    fn old_projects_without_layers_remain_readable() {
        let json = r#"{"schemaVersion":1,"engineVersion":"0.1.0","source":{"path":"photo.jpg","contentHash":"abc","byteLength":42},"globalAdjustments":{"exposureEv":0.0,"contrast":0.0,"highlights":0.0,"shadows":0.0,"whites":0.0,"blacks":0.0,"temperature":0.0,"tint":0.0,"vibrance":0.0,"saturation":0.0},"masks":[]}"#;
        let restored: Project = serde_json::from_str(json).expect("deserialize old project");
        assert!(restored.layers.is_empty());
        assert!(restored.camera_profile.is_none());
    }

    #[test]
    fn sidecar_upgrade_boundary_is_atomic_typed_and_failure_safe() {
        let root =
            std::env::temp_dir().join(format!("starroom-project-m30-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("legacy.starroom.json");
        let legacy = r#"{"schemaVersion":1,"engineVersion":"0.1.0","source":{"path":"photo.jpg","contentHash":"abc","byteLength":42},"globalAdjustments":{"exposureEv":0.0,"contrast":0.0,"highlights":0.0,"shadows":0.0,"whites":0.0,"blacks":0.0,"temperature":0.0,"tint":0.0,"vibrance":0.0,"saturation":0.0},"masks":[]}"#;
        fs::write(&path, legacy).unwrap();
        let loaded = Project::read_sidecar(&path).unwrap();
        assert_eq!(loaded.schema_version, 1);
        loaded.write_sidecar(&path).unwrap();
        assert_eq!(Project::read_sidecar(&path).unwrap().schema_version, 1);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);

        let future_path = root.join("future.starroom.json");
        fs::write(
            &future_path,
            legacy.replace("\"schemaVersion\":1", "\"schemaVersion\":999"),
        )
        .unwrap();
        let future_before = fs::read(&future_path).unwrap();
        assert!(matches!(
            Project::read_sidecar(&future_path),
            Err(ProjectError::UnsupportedSchema(999))
        ));
        assert_eq!(fs::read(&future_path).unwrap(), future_before);

        let corrupt_path = root.join("corrupt.starroom.json");
        fs::write(&corrupt_path, b"{not-json").unwrap();
        let corrupt_before = fs::read(&corrupt_path).unwrap();
        assert!(matches!(
            Project::read_sidecar(&corrupt_path),
            Err(ProjectError::Serialize(_))
        ));
        assert_eq!(fs::read(&corrupt_path).unwrap(), corrupt_before);
    }

    #[test]
    fn layers_reject_duplicate_identity_and_invalid_opacity() {
        let layer = AdjustmentLayer {
            id: "duplicate".into(),
            name: "Layer".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            order: 0,
            mask: MaskDefinition::None.into(),
            adjustments: BTreeMap::new(),
        };
        let project = Project {
            schema_version: 2,
            engine_version: "0.2.0".into(),
            source: SourceIdentity {
                path: "fixture.jpg".into(),
                content_hash: "hash".into(),
                byte_length: 1,
            },
            global_adjustments: GlobalAdjustments::default(),
            camera_profile: None,
            white_balance: PersistedWhiteBalance::default(),
            tone_curves: PersistedToneCurves::default(),
            color_mixer: PersistedColorMixer::default(),
            color_grading: PersistedColorGrading::default(),
            detail: PersistedDetail::default(),
            optics: PersistedOptics::default(),
            geometry: PersistedGeometry::default(),
            skin_retouch: PersistedSkinRetouch::default(),
            healing_operations: vec![],
            masks: vec![],
            layers: vec![layer.clone(), layer],
        };
        assert!(matches!(
            project.validate_layers(),
            Err(ProjectError::InvalidLayers(_))
        ));
    }

    #[test]
    fn m15_extended_mask_leaves_round_trip_without_rasterizing() {
        let tree = MaskTree::Composite(MaskComposite {
            operation: MaskOperation::Add,
            children: vec![
                MaskDefinition::Linear {
                    start_x: 0.1,
                    start_y: 0.2,
                    end_x: 0.8,
                    end_y: 0.6,
                    feather: 0.15,
                    invert: false,
                }
                .into(),
                MaskDefinition::ColorRange {
                    reference: [0.35, 0.42, 0.51],
                    tolerance: 0.08,
                    feather: 0.12,
                    invert: true,
                }
                .into(),
                MaskTree::Composite(MaskComposite {
                    operation: MaskOperation::Invert,
                    children: vec![
                        MaskDefinition::Luminance {
                            minimum: 0.1,
                            maximum: 0.7,
                            feather: 0.04,
                            invert: false,
                        }
                        .into(),
                    ],
                }),
            ],
        });
        let json = serde_json::to_string(&tree).expect("serialize extended mask");
        let restored: MaskTree = serde_json::from_str(&json).expect("deserialize extended mask");
        assert_eq!(restored, tree);
    }

    #[test]
    fn m16_portrait_semantic_leaf_persists_model_and_refinement_identity() {
        let tree: MaskTree = MaskDefinition::PortraitSemantic {
            face_id: "face-cafebabe".into(),
            region: PortraitMaskRegion::Skin,
            threshold: 0.55,
            feather: 0.08,
            model_id: "yakhyo/face-parsing-bisenet-resnet18".into(),
            model_version: "8a4729d".into(),
            model_hash: "a".repeat(64),
            cache_key: "face-cafebabe:transform".into(),
        }
        .into();
        let restored: MaskTree =
            serde_json::from_str(&serde_json::to_string(&tree).expect("serialize"))
                .expect("deserialize");
        assert_eq!(restored, tree);
    }

    #[test]
    fn m20_generated_mask_node_round_trips_without_raster_pixels() {
        let tree: MaskTree = MaskDefinition::Generated {
            provider_id: "foreground".into(),
            model_id: "birefnet-subject".into(),
            model_version: "v1/pinned".into(),
            model_hash: "b".repeat(64),
            semantic_class: GeneratedMaskSemantic::Subject,
            threshold: 0.5,
            feather: 0.08,
            invert: false,
            cache_identity: "cache-subject".into(),
            metadata: BTreeMap::from([("executionProvider".into(), "cpu".into())]),
        }
        .into();
        let json = serde_json::to_string(&tree).expect("serialize");
        assert!(!json.contains("values"));
        assert_eq!(
            serde_json::from_str::<MaskTree>(&json).expect("restore"),
            tree
        );
    }
}
