# M19 — Deterministic Semantic Advisor

## Native analysis

`starroom-advisor` derives histogram bins, ordered luminance percentiles, black/white clipping, contrast, chroma and white-balance estimates from Native working data. When the M16 cache is available it also reports skin-weighted portrait luminance, chroma and sample fraction. Missing portrait analysis is explicit optional state, not fabricated data.

## Explainable rules

Every recommendation contains a condition-derived parameter, bounded delta, confidence, human-readable reason and priority. Conflict handling and safety limits are deterministic for identical statistics. The system has no LLM, GPT, remote API, telemetry or learned recommendation model.

UI actions are Preview, Apply, Ignore, Dismiss and safe Apply All. Preview stages reversible edit state and renders through the normal Native graph; acceptance commits it to existing snapshot history.

## Acceptance

Synthetic fixtures cover dark exposure, white clipping, warm bias, shadow/black-anchor conflict, ordered finite percentiles, bounded recommendations and deterministic priority. Portrait statistics exercise the same M16 semantic cache used by retouch.
