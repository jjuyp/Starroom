// Explicit heavy/opt-in Native request benchmark, not an installed UI latency gate.
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'

const format = process.argv.find((argument) => argument.startsWith('--format='))?.slice(9) ?? 'rgba8'
if (!['jpeg', 'rgba8'].includes(format)) throw new Error('Choose --format=jpeg or --format=rgba8')
const compiler = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).trim()
if (process.platform !== 'win32' || !/host: .*windows-msvc/.test(compiler)) {
  throw new Error('This production timing gate requires Windows MSVC; secondary targets are not equivalent')
}
const sourceFiles = ['Cargo.lock', 'src-tauri/src/lib.rs', 'src-tauri/src/preview_transport.rs',
  'crates/starroom-pipeline/src/lib.rs', 'crates/starroom-detail/src/lib.rs',
  'crates/starroom-look/src/lib.rs', 'crates/starroom-render/src/profiling.rs',
  'crates/starroom-color-management/src/lib.rs', 'crates/starroom-color-management/src/gamut.rs']
const sourceIdentity = () => Object.fromEntries(sourceFiles.map((path) => [path,
  createHash('sha256').update(readFileSync(path, 'utf8').replaceAll('\r\n', '\n')).digest('hex')]))
const sources = sourceIdentity()
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
const dirty = execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim() !== ''
const args = ['test', '--locked', '--release', '-p', 'starroom-desktop',
  'rc2_release_preview_cache_interaction_and_viewport_timings', '--', '--ignored', '--nocapture']
const result = spawnSync('cargo', args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
  env: { ...process.env, STARROOM_RC2_PERFORMANCE_GATE: '1', STARROOM_PREVIEW_PERF_FORMAT: format } })
const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`
process.stdout.write(output)
const controls = [...output.matchAll(/NATIVE_CONTROL_PERF control=(\S+) changed_samples=(\d+) edge=(\d+) median_ms=([\d.]+) p95_ms=([\d.]+)/g)]
  .map((match) => ({ control: match[1], changedSamples: Number(match[2]), edge: Number(match[3]),
    medianMs: Number(match[4]), p95Ms: Number(match[5]) }))
const changedDuringRun = JSON.stringify(sources) !== JSON.stringify(sourceIdentity())
const failure = changedDuringRun ? 'Source changed during compilation/measurement; retain report but do not accept it'
  : result.error?.message ?? (result.status !== 0 ? `cargo exit ${result.status}`
  : controls.length !== 27 ? `Expected27 controls, received${controls.length}` : null)
const report = { schemaVersion: 1, generatedAt: new Date().toISOString(), revision, dirty, format, compiler,
  sources, changedDuringRun,
  scope: 'Real MSVC Release Native requests on24MP synthetic source, excludes Tauri IPC/browser/OS presentation',
  command: ['cargo', ...args], exitCode: result.status, failure, controls, output }
mkdirSync('.starroom-reports', { recursive: true })
const path = `.starroom-reports/native-preview-${format}-${Date.now()}.json`
writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' })
console.log(`Native preview performance report: ${path}`)
if (failure) { console.error(failure); process.exitCode = 1 }
