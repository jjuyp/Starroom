# Third-party notices

## Adobe DNG technology

This product includes DNG technology under license by Adobe.

Starroom's M4 scene-linear tone operator contains GPL-derived / private-use
adaptations of darktable's sigmoid module. Source: darktable-org/darktable,
release-5.6.0, commit 3c17b2976793303c186a5f64e8c9635ecf8b15d3,
src/iop/sigmoid.c. Original license: GPL-3.0-or-later. Starroom is made
available under GPL-3.0-or-later; see the upstream GPL-3.0 text and the
provenance inventory for source and modification details.

Starroom reads and processes DNG files through its LibRaw integration and the
documented DNG metadata boundary. The applicable DNG patent-license terms are
recorded in `docs/17_THIRD_PARTY_PROVENANCE.md`.

## Other notices

Starroom embeds the unmodified Lensfun v0.3.4 lens database at commit
101c745e847a5de4a1e569a94368ce2027198598 under CC-BY-SA-3.0 and adapts
Lensfun's LGPL-3.0-or-later Poly3, Poly5 and PTLens modifier equations in the
Rust optics provider. The retained database license is located beside the data
under `crates/starroom-optics/data/lensfun-v0.3.4`.

Third-party source, version, license and packaging records are maintained in
`docs/17_THIRD_PARTY_PROVENANCE.md`. The selected LibRaw CDDL-1.0 license and
source are retained under `vendor/libraw-0.22.2`; the retained
colour-science BSD-3-Clause notice accompanies its test fixture.
