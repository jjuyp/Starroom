# Preview pixels and layout share one publication boundary

## Observed code defect

`PreviewCanvas` published Native `sourceWidth/sourceHeight` immediately after IPC,
before asynchronous bitmap/JPEG decode and generation validation. An old response
could therefore update the 100% canvas CSS dimensions even though its pixels were
later rejected. Native completion is not successful presentation.

## Repair

The existing latest-frame predicate remains unchanged. A single synchronous
`publishCurrentPreviewFrame` boundary also checks photo and Before/After identity,
then executes the real canvas presentation and its metadata updates together.
Source dimensions are updated only after a successful draw. Unchanged dimensions
reuse the previous React state object. Published generation is acknowledged after
successful presentation, not before it. Active-photo identity is refreshed in the
layout effect; deferred paint measurements also check photo identity.

Both fit and Native tile presentation use this boundary. No RAW decode, image
math, sampling, geometry, ICC, final-quality resolution or Export behavior changes.
Same-source interactive intermediate frames remain allowed to avoid starvation;
older decoded frames cannot overwrite a published newer frame or a final request.

## Evidence and limits

Targeted publication/presentation/latency tests:17 passed. New regressions cover
delayed decode with newer/final publication, photo change, Before/After change,
interactive ordering and draw failure propagation. Full Web:245 tests/35files,
report1794316169140; six selected Golden manifest cases and immutable photographic
source checks pass. TypeScript and ESLint pass. Canonical Windows ASCII-alias
production build generated fresh dist assets on2026-11-10 (direct Unicode-path
Vite exited without a new output and was not accepted as build evidence).

This is a race repair, not evidence that all rotation/crop/DPI geometry or
installed latency requirements are complete. Real Windows UI and the full final
release matrix remain required. No numeric tolerance or existing test was removed.
