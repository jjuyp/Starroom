# Third-party notices

LittleCMS Fast Float 2.19, copyright (c) 1998-2026 Marti Maria Saguer,
is included under GPL-3.0-or-later (not the core LittleCMS MIT license).
Unmodified source: mm2/Little-CMS commit
21c582a594fe5279f90c0b93437c398f93bf62b0, plugins/fast_float.
Retained source, hashes and full license: vendor/lcms-fast-float-2.19.
Redistributed binaries must retain these notices and corresponding GPL source.

Starroom's Native correlated-temperature provider is a BSD-3-Clause-derived scalar
port of Colour Developers' Robertson (1968) implementation, colour-science/colour
v0.4.7, colour/temperature/robertson1968.py, blob 380b8bf8327098e8532eedd219084e8386547530.
Copyright 2013 Colour Developers. The full retained notice is
crates/starroom-color-management/COLOUR_SCIENCE_LICENSE.txt. No endorsement is implied.

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

Starroom modifies LibRaw 0.22.2 `src/metadata/identify.cpp` (2026-10-10) to promote
DNG matrix/illuminant presence flags alongside the selected IFD values. That
covered modification remains available under CDDL-1.0 in the corresponding source.
See `vendor/libraw-0.22.2/STARROOM_MODIFICATIONS.md`; no demosaic algorithm is changed.

The 2026-10-11 covered modification additionally extends the existing DNG metadata
parser/types and selected-IFD coefficient paths with exact calibration signature
matching. All changed covered sources remain available under CDDL-1.0 with retained
copyrights and explicit source/hash records in STARROOM_MODIFICATIONS.md.
