// Explicit heavy/opt-in Native request benchmark, not an installed UI latency gate.
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { extname } from 'node:path'

const format = process.argv.find((argument) => argument.startsWith('--format='))?.slice(9) ?? 'rgba8'
const prime = process.argv.includes('--prime')
const combined = process.argv.includes('--combined')
if (!['jpeg', 'rgba8'].includes(format)) throw new Error('Choose --format=jpeg or --format=rgba8')
const localSourceArgument = process.argv.find((argument) => argument.startsWith('--source='))
const localSource = localSourceArgument?.slice(9)
const dimensions = process.argv.find((argument) => argument.startsWith('--dimensions='))?.slice(13)
if (localSourceArgument && (!localSource || format !== 'rgba8' || !/^\d+x\d+$/.test(dimensions ?? ''))) {
  throw new Error('Read-only local source requires --format=rgba8 and explicit --dimensions=WIDTHxHEIGHT')
}
if (combined && !localSource) throw new Error('--combined requires an explicit read-only original and CPU/export comparison')
const privateIdentity = () => localSource ? {
  kind: 'private-local-original-never-redistributed', format: extname(localSource).toLowerCase(), dimensions,
  sha256: createHash('sha256').update(readFileSync(localSource)).digest('hex'),
} : { kind: 'generated24MP' }
const sourcePhoto = privateIdentity()
const compiler = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).trim()
if (process.platform !== 'win32' || !/host: .*windows-msvc/.test(compiler)) {
  throw new Error('This production timing gate requires Windows MSVC; secondary targets are not equivalent')
}
const sourceFiles = ['Cargo.lock', 'src-tauri/src/lib.rs', 'src-tauri/src/preview_transport.rs',
  'crates/starroom-pipeline/src/lib.rs', 'crates/starroom-pipeline/src/source_preparation.rs',
  'crates/starroom-pipeline/src/cancellation.rs',
  'scripts/native-preview-performance.mjs', 'crates/starroom-detail/src/lib.rs',
  'crates/starroom-look/src/lib.rs', 'crates/starroom-render/src/profiling.rs',
  'crates/starroom-imageio/src/lib.rs', 'crates/starroom-imageio/src/unbounded_resize.rs',
  'crates/starroom-raw/src/lib.rs',
  'crates/starroom-color-management/src/lib.rs', 'crates/starroom-color-management/src/gamut.rs']
const sourceIdentity = () => Object.fromEntries(sourceFiles.map((path) => [path,
  createHash('sha256').update(readFileSync(path, 'utf8').replaceAll('\r\n', '\n')).digest('hex')]))
const sources = sourceIdentity()
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
const dirty = execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim() !== ''
const args = ['test', '--locked', '--release', '-p', 'starroom-desktop',
  'rc2_release_preview_cache_interaction_and_viewport_timings', '--', '--ignored', '--nocapture']
const result = spawnSync('cargo', args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
  env: { ...process.env, STARROOM_RC2_PERFORMANCE_GATE: '1', STARROOM_PREVIEW_PERF_FORMAT: format,
    STARROOM_PREVIEW_PERF_PRIME: prime ? '1' : '',
    STARROOM_PREVIEW_PERF_COMBINED: combined ? '1' : '',
    ...(localSource ? { STARROOM_PREVIEW_PERF_SOURCE: localSource, STARROOM_PREVIEW_PERF_DIMENSIONS: dimensions }
      : { STARROOM_PREVIEW_PERF_SOURCE: '', STARROOM_PREVIEW_PERF_DIMENSIONS: '' }) } })
const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`
process.stdout.write(output)
const controls = [...output.matchAll(/NATIVE_CONTROL_PERF control=(\S+) changed_samples=(\d+) edge=(\d+) median_ms=([\d.]+) p95_ms=([\d.]+)/g)]
  .map((match) => ({ control: match[1], changedSamples: Number(match[2]), edge: Number(match[3]),
    medianMs: Number(match[4]), p95Ms: Number(match[5]) }))
const combinedControls = [...output.matchAll(/NATIVE_COMBINED_PERF control=(\S+) changed_samples=(\d+) edge=(\d+) median_ms=([\d.]+) p95_ms=([\d.]+)/g)]
  .map((match) => ({ control: match[1], changedSamples: Number(match[2]), edge: Number(match[3]),
    medianMs: Number(match[4]), p95Ms: Number(match[5]) }))
const firstMatch = output.match(/NATIVE_FIRST_INTERACTIVE cold_tier_ms=([\d.]+) profile=(\{[^\r\n]+\})/)
const firstInteractive = firstMatch ? { elapsedMs: Number(firstMatch[1]), profile: JSON.parse(firstMatch[2]) } : null
const parityMatch = output.match(/NATIVE_LOCAL_PARITY compared_frames=(\d+) max_rgb8_delta=(\d+)/)
const localParity = parityMatch ? { comparedFrames: Number(parityMatch[1]), maxRgb8Delta: Number(parityMatch[2]) } : null
const primingMatch = output.match(/NATIVE_PRIMING_READY elapsed_ms=([\d.]+)/)
const priming = primingMatch ? { elapsedMs: Number(primingMatch[1]), scope: 'completed cache preparation, not UI scheduling' } : null
const changedDuringRun = JSON.stringify(sources) !== JSON.stringify(sourceIdentity())
const failure = changedDuringRun ? 'Source changed during compilation/measurement; retain report but do not accept it'
  : JSON.stringify(sourcePhoto) !== JSON.stringify(privateIdentity()) ? 'Original source bytes changed'
  : result.error?.message ?? (result.status !== 0 ? `cargo exit ${result.status}`
  : controls.length !== 27 ? `Expected27 controls, received${controls.length}`
    : combinedControls.length !== (combined ? 4 : 0) ? 'Combined control timing did not complete'
    : prime && !priming ? 'Missing completed Native priming evidence'
    : !firstInteractive ? 'Missing actual first-interactive cold-tier measurement'
      : localSource && (!localParity || localParity.comparedFrames !== (combined ? 157 : 137) || localParity.maxRgb8Delta > 1)
        ? 'Actual local photo CPU/GPU parity did not complete' : null)
const report = { schemaVersion: 1, generatedAt: new Date().toISOString(), revision, dirty, format, compiler,
  sources, changedDuringRun, sourcePhoto, localParity, priming,
  scope: `Real MSVC Release Native requests on24MP ${localSource ? 'private read-only original' : 'synthetic source'}, excludes Tauri IPC/browser/OS presentation`,
  command: ['cargo', ...args], exitCode: result.status, failure, firstInteractive, controls, combinedControls, output }
mkdirSync('.starroom-reports', { recursive: true })
const path = `.starroom-reports/native-preview-${format}-${Date.now()}.json`
writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' })
console.log(`Native preview performance report: ${path}`)
if (failure) { console.error(failure); process.exitCode = 1 }
