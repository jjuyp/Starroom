# GPU hardware timing and host pixel fingerprint path

2026-10-15. This is targeted production instrumentation/performance evidence, not completion
of GPU-resident presentation, all interaction latency or Final Acceptance.

## Foundation / implementation

Uses the existing pinned wgpu 30.0.1 public API:
[compute-pass timestamp writes](https://docs.rs/wgpu/30.0.1/wgpu/struct.ComputePassTimestampWrites.html)
and [queue timestamp period](https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html#method.get_timestamp_period).
No dependency, shader/image math, output precision or processing order is changed.

- Request TIMESTAMP_QUERY only when advertised by the adapter. Unsupported hardware keeps
  acceleration available without timestamps; `timestampQuerySupported=false` is explicit.
- Real exposure and fused-creative passes write beginning/end GPU timestamps. A persistent
  16-byte resolve buffer and 16-byte tail of the existing staging buffer carry the pair.
  There is still exactly one command submission, one map/poll and one final readback per render,
  not a second timestamp wait or full-frame copy. Reported buffer bytes include the extra32;
  driver/query-set physical memory remains outside that sum.
- Resolve/copy occurs only for an executed pass. A creative cache hit has no new hardware
  duration/pass count, never a stale duration from its earlier dispatch. Smaller frames reusing
  larger resources place the pair after the current pixel range, not after maximum capacity.
- `gpuHardwareNanoseconds`/`gpuHardwarePassCount` are separate from legacy host-elapsed GPU
  fields. Missing/unsupported/invalid or reversed timestamps are None, never a host-time guess.
  Tick delta uses the queue's nanoseconds-per-tick period. No presentation/queue/upload/ICC
  duration is falsely assigned to GPU compute. Fused passes are not falsely split per operator.

## Actual DX12 evidence before host-hash optimization

MSVC release, NVIDIA RTX3050 Laptop, immutable public X-Trans RAW 1024x680, prepared-source
cache, five measured iterations after warm-up. Report
`.starroom-reports/color-latency-1792076124929-20712.json`, kernel label
`32741ad+uncommitted-hardware-timestamps`. CPU reference is outside the timed GPU interval.
Values below are the **last measured request's** stage fields, not median stage timings.

| Control | Actual GPU pass ms | Host creative-call ms | Readback host ms | Output CMM host ms |
|---|---:|---:|---:|---:|
| Exposure | .128000 | 17.8345 | 3.3104 | 27.8833 |
| Relative Temperature | .126976 | 16.9573 | 3.1985 | 28.4643 |
| Saturation | .128000 | 17.4293 | 3.3801 | 25.8759 |
| Physical Tint | .125952 | 20.6785 | 3.8967 | 27.3016 |

Exposure engine median/p95 is62.8004/68.6656ms; physical Tint69.9035/92.7526ms. RGB8 CPU/GPU
max differences remain1/0 respectively. Source upload count20 / final readback54 for54 requests
is unchanged. Auto-WB's retained creative output records None and zero executed GPU passes,
while still truthfully recording its required host/output/readback work.

This shows host preparation/transfer/ICC dominate this sample, **not** that all GPU kernels,
RAW decode, UI or display presentation are fast. No comparison against Lightroom is claimed.

## Bit-exact host fingerprint optimization

The source cache still hashes actual pixel bytes; caller keys are not trusted as proof of pixel
identity. The old per-u32 Hasher calls become a single contiguous `Hasher::write` over the same
native-endian byte stream. This uses Rust's existing mature standard hash implementation,
not a weaker hash or a change to source/profile/settings keys. Existing cache fingerprints
remain bit-exact on tested targets; tests compare previous/new implementations across empty,
small, odd and1024x680 frames, signed zero, negative/HDR values and preserved NaN bit patterns.
No screenshot-only or mock renderer substitutes the production path.

Actual MSVC release microbenchmark (same process, seven timings each, median,1024x680 RGBA):
previous per-component calls9.3582ms; contiguous byte stream2.6002ms. The same test asserts
exact fingerprint equality; timing is evidence, not a flaky speed threshold in ordinary CI.
Debug timings113.1120/44.1279ms are recorded separately and must not be used as release speed.

Real Native RAW remeasurement is report`color-latency-1792076553859-15324.json`, kernel label
`32741ad+uncommitted-hardware-timestamps-bulk-hash`:

| Control | Previous engine median/p95 ms | Contiguous-hash engine median/p95 ms | Max RGB8 delta |
|---|---:|---:|---:|
| Exposure |62.8004 /68.6656 |56.5347 /66.8201 |1 |
| Relative Tint |61.9320 /77.0716 |56.9782 /60.4817 |1 |
| Saturation |60.4933 /60.9066 |57.1630 /61.5034 |1 |
| Neutral Picker |71.2858 /77.3291 |61.1641 /67.9366 |0 |
| Physical Temperature |73.3862 /73.7457 |62.7689 /64.6592 |0 |
| Physical Tint |69.9035 /92.7526 |64.4941 /68.0181 |0 |

Exposure's last host creative call changes17.8345→9.0194ms while hardware pass remains
.128000→.126976ms; last CMM27.8833→28.2175ms does not improve. Saturation p95 is slightly
worse, so do not claim every percentile improved. Both runs include other validation/build
activity on the machine; these five-sample engine observations are not an isolated system-
wide performance acceptance or a substitute for quiet-machine pointer-to-present testing.
Source bytes and all54 CPU/GPU comparisons remain valid at the original one-code bound.

A second run after this task's build/test processes finished is retained, not discarded:
`color-latency-1792076851022-17280.json` (`...bulk-hash-quiet`). Exposure56.9682/58.6367ms
and physical Temperature60.6402/62.9823ms median/p95, but Saturation p95140.4390ms,
Neutral Picker103.7217/135.4593ms and physical Tint70.0710/181.4273ms. Other system/driver
activity was not controlled. These are genuine outliers/failures of a universal <100ms engine
claim; **the latency requirement is not passed**, even before UI/encoding/presentation time.
Do not keep rerunning until a favorable sample hides this report. Continue targeted latency
work and statistically stronger field measurement.

Targeted real-GPU tests compare measured/unmeasured devices' actual output exactly, ensure
four calls still produce four readbacks, reject stale timings on cache hits, and cover resized
capacity. Existing poison/device loss/OOM/liveness/CPU parity tests remain intact. Timing
conversion rejects malformed buffers, invalid period, reverse counts and overflow. Final local
Full Rust passes report1792076774130 (455 registered/451 passed/four opt-in excluded), including
the final hash implementation and actual RAW/photographic/GPU/export/recovery tests. Format
and warning-denied workspace Clippy pass. Frontend source is unchanged from the211-test Full
Web report1792075555690. New remote acceptance is required after commit/push; no Final gate
is satisfied merely by these ordinary Full results.

Remaining requirements: remove unnecessary CPU copies/readback, real direct presentation,
correct destination color management at that surface, actual pointer-to-present, downstream
caches/tiles, full24/100MP quality/memory/performance and same-SHA Windows/Final gates.
