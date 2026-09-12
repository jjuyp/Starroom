# M29 Production UX, Reliability and Desktop Hardening

## One command architecture

`src/commands.ts` is the single catalog for keyboard shortcuts and the Command Palette. Both
surfaces resolve to a `CommandId` and invoke `App.executeCommand`; there is no second action path.
The catalog covers Undo/Redo, copy/paste settings, Before/After, Mask, Healing, Crop, rating,
Pick/Reject, Fit, 1:1, Filmstrip, panels and Export. `Ctrl/Cmd+K` opens a searchable modal whose
focus is trapped and restored to its prior control on close.

## Session safety

`starroom-session` writes a versioned, pixel-free `SessionState` through a same-directory
`NamedTempFile`, flushes it, and atomically persists it. Autosave marks the envelope as interrupted;
normal close rewrites the same state as clean. Startup never guesses: interrupted state presents
Recover and Discard, while a clean state restores workspace, selected asset/path, tool, panels,
filmstrip, zoom and Library context. Corrupt, future or invalid session data is a typed error.
If the clean-session write fails, the close request is cancelled, the diagnostic is shown and the
interrupted recovery envelope is retained; Starroom never destroys the window after a failed save.

Native History remains the durable edit record. Browser fallback edits are explicitly transient;
closing with such edits requires confirmation. Neither autosave nor session restore serializes source
pixels or overwrites source photos.

## Desktop input and errors

The native window listens to Tauri 2 drag/drop events. JPEG, PNG, TIFF, NEF, ARW, CR2, CR3, DNG and
RAF paths enter the Native preview contract. Unsupported drops are rejected visibly and never become
a silent Browser fallback. File and export pickers retain their existing native paths.

Typed failures are mapped to File, RAW, Color, Library, Missing, Relink, AI, Export, Memory,
Permission and Session messages while retaining diagnostic detail. Export cancellation, partial
failure and progress remain visible.

## DPI, accessibility and responsive behavior

All image-space pointer input is normalized from `getBoundingClientRect()` CSS pixels. The shared
mapper guards zero-sized surfaces, clamps input and is regression-tested at equivalent 100% and 200%
scales. This prevents device-pixel ratio from being applied twice when moving between monitors.

The UI provides semantic navigation/regions, named controls, visible `:focus-visible` treatment,
keyboard access, modal semantics, tooltips and `prefers-reduced-motion`. Responsive gates preserve
the professional workspace at desktop widths and provide a clear minimum-size state rather than a
misaligned canvas.

## Offline and workflow evidence

Production source contains no HTTP client, `fetch`, WebSocket, telemetry or analytics path. AI model
providers load only hash-pinned local files; missing weights are typed unavailable states. A
Playwright desktop-width audit observed only the local development origin and verified command
search/execution, focus containment and responsive layout. The Native integration regression covers
Import -> Library -> rating/keyword/collection -> edit/history -> Snapshot -> Export -> clean session
reopen -> deterministic Export again, while the existing missing/relink and undo/redo scenarios stay
active.

Installer runtime, clean-machine startup and cross-monitor physical-machine qualification remain M30
release gates; they are not replaced by this M29 browser audit.
