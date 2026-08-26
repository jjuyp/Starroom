# Starroom third-party notices

Starroom is licensed GPL-3.0-or-later. The authoritative dependency, source revision, integration
mode, binary inclusion and redistribution-risk inventory is
[`docs/17_THIRD_PARTY_PROVENANCE.md`](docs/17_THIRD_PARTY_PROVENANCE.md). Model code and weight
licenses are separately recorded in [`MODEL_PROVENANCE.md`](MODEL_PROVENANCE.md).

The release includes or links code from LibRaw (CDDL-1.0 selected), LittleCMS and Rust wrappers
(MIT), Lensfun code/database (LGPL-3.0-or-later / CC-BY-SA-3.0), Tauri (MIT OR Apache-2.0), wgpu
(MIT OR Apache-2.0), ONNX Runtime Rust bindings (MIT OR Apache-2.0), SQLite (public domain) and the
codec/dependency closure enumerated in the provenance inventory. Retain all upstream copyright,
license and notice files in source and binary distributions.

The product contains a GPL-derived adaptation from darktable `sigmoid.c`; source/revision and
modifications are recorded in the provenance inventory and `NOTICE.md`. Distributors must satisfy
GPL corresponding-source obligations for the complete covered work. The DNG notice required by the
Adobe patent license is retained in `NOTICE.md`.

Neural weights under `models/local/` are intentionally untracked and unbundled. In particular,
BiSeNet, SegFormer and NAFNet checkpoint redistribution has not been cleared for this public binary.
An installer without those exact local weights must report the corresponding AI capability as
unavailable and must not download or substitute a model silently.
