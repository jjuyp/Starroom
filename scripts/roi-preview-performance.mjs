import { createHash } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'

// Opt-in measurement, not a substitute for installed input-to-display acceptance.
const rust = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).trim()
if (process.platform !== 'win32' || !/host: .*windows-msvc/.test(rust)) {
  throw new Error('ROI timing requires Windows MSVC; secondary targets are not release evidence')
}
const files = ['Cargo.lock', 'src-tauri/src/lib.rs', 'crates/starroom-imageio/src/lib.rs',
  'scripts/roi-preview-performance.mjs', 'fixtures/golden/sources/neon-streets-nikon-d7100.jpg']
const identities = () => Object.fromEntries(files.map((file) => [file,
  createHash('sha256').update(readFileSync(file)).digest('hex')]))
const before = identities()
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
const dirty = execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim() !== ''
const result = spawnSync('cargo', ['test', '--release', '--locked', '-p', 'starroom-desktop',
  '--lib', 'encoded_roi_cache_real_codec_timing', '--', '--ignored', '--nocapture'], {
  encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
  env: { ...process.env, STARROOM_ROI_CACHE_GATE: '1' },
})
const output = (result.stdout ?? '') + (result.stderr ?? '')
process.stdout.write(output)
const match = output.match(/ROI_CACHE_PERF source=(\d+)x(\d+) roi=(\d+)x(\d+) comparisons=(\d+) cold_median_ms=([\d.]+) cold_p95_ms=([\d.]+) warm_median_ms=([\d.]+) warm_p95_ms=([\d.]+)/)
const unchanged = JSON.stringify(before) === JSON.stringify(identities())
const measurements = match ? {
  source: [Number(match[1]), Number(match[2])], roi: [Number(match[3]), Number(match[4])],
  comparisons: Number(match[5]), coldMedianMs: Number(match[6]), coldP95Ms: Number(match[7]),
  warmMedianMs: Number(match[8]), warmP95Ms: Number(match[9]),
} : null
const passed = result.status === 0 && unchanged && measurements?.comparisons === 22
  && measurements.warmMedianMs < measurements.coldMedianMs
mkdirSync('.starroom-reports', { recursive: true })
const path = `.starroom-reports/roi-preview-${Date.now()}.json`
writeFileSync(path, JSON.stringify({
  schemaVersion: 1, generatedAt: new Date().toISOString(), revision, dirty, rust, sources: before,
  scope: 'Actual JPEG ROI decode/cache plus 22 changed-exposure CPU shared-graph comparisons; timings exclude graph, UI, IPC, GPU and OS presentation',
  unchanged, exitCode: result.status, measurements, passed,
}, null, 2) + '\n')
console.log(`ROI performance report: ${path}`)
if (!passed) process.exitCode = 1
