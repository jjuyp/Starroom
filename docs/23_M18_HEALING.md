# M18 — Native Healing and Shared Brush Input

## Operation model

Each `HealingOperation` is immutable project intent: Clone or Heal mode, auto/manual source, source/target position, radius, feather, opacity, rotation, scale, tone adaptation and texture adaptation. Clone copies a transformed patch; Heal transfers texture while adapting local tone. Generative inpainting is deliberately unavailable and returns a typed error.

All coordinates are normalized source-image coordinates. Rendering occurs in `starroom-heal` inside the Native shared graph, so Preview and Export evaluate the same ordered operations without changing the source photo. Operation serialization participates in graph/cache identity.

## Shared brush interaction

The preview canvas records continuous freehand strokes in source-normalized coordinates. Interpolated points keep spacing stable under zoom; radius, hardness, flow/opacity and erase intent stay compact edit state. M15 brush masks and Healing consume this grammar, and M20 refinement can compose it through existing mask algebra.

## Acceptance

Regressions cover zero-opacity identity, spot reduction, edge safety, Clone/Heal determinism, auto-source determinism, finite output, unsupported inpainting and Preview/Export parity. Existing snapshot state supplies undo/redo.
