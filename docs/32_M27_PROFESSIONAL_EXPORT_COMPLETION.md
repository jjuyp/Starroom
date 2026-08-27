# M27 Professional Export Completion

## Precision contract

The Native shared graph stays `f32` from decoded source through linear Rec.2020 D65 creative
processing and the selected LittleCMS output transform. `RenderedRgbF32` is an encoded-output-space
surface, not a second graph. The export crate resizes and output-sharpens this float buffer, then
quantizes exactly once to RGB8 or RGB16 at the codec boundary.

JPEG supports RGB8 only. A 16-bit JPEG request is `UnsupportedBitDepth`; it never downgrades. PNG
and TIFF accept RGB8 and RGB16. Decoder round-trip regressions assert the codec reports RGB16 and a
1,024-step gradient retains more than 256 distinct levels, preventing 8-bit up-conversion from
passing acceptance.

## Colour management

sRGB, Display P3, Adobe RGB and Rec.2020 continue through the existing LittleCMS provider. The
provider runs once, before quantization. The exact selected profile bytes are embedded when enabled;
invalid profiles remain typed failures. Preview continues to use the same graph with its display
output surface, while export selects a file output profile.

## Output sharpening

`Off`, `Screen` and `Print` are separate typed targets. Both active modes use deterministic Gaussian
unsharp masking on the float output buffer. Screen uses a small display-oriented radius. Print uses
a larger radius/gain derived from final megapixels and the source-to-output resize ratio, modelling
resampling and print-dot spread. Low/Standard/High scale the mode-specific gain and are not aliases.

## Metadata and privacy

EXIF capture/camera/copyright fields follow the selected metadata policy. Rating, keywords,
copyright, capture date and camera model are written in an XMP packet; `dc:subject` supplies the
interoperable IPTC keyword representation. JPEG uses standard XMP APP1, PNG uses an XMP iTXt chunk,
and TIFF uses tag 700.

GPS is excluded by default. `Preserve Location` only forwards real source EXIF when available; a
source without transferable location metadata returns `MetadataWriteFailed`. Starroom never creates
coordinates.

## Safety and scale

Existing immutable-source, batch isolation, cancellation, filename/collision and atomic
temp/fsync/rename behaviour remains authoritative. Preflight covers 24/45/60/100 MP classes using
the conservative float graph budget. Cancellation before atomic rename leaves no partial output.

## Acceptance evidence

The M27 acceptance commit and final GitHub Actions URL are recorded in `TODO.md` and
`codex/CURRENT_TASK.md` only after the full milestone gate succeeds.
