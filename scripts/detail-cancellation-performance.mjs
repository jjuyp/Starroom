import { createHash } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'

const compiler = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).trim()
if (process.platform !== 'win32' || !/host: .*windows-msvc/.test(compiler)) throw new Error('Windows MSVC required')
const paths = ['Cargo.lock', 'crates/starroom-detail/src/lib.rs', 'crates/starroom-detail/examples/cancellation_probe.rs',
  'crates/starroom-pipeline/src/lib.rs', 'crates/starroom-pipeline/src/cancellation.rs', 'scripts/detail-cancellation-performance.mjs']
const identity = () => Object.fromEntries(paths.map((path) => [path, createHash('sha256')
  .update(readFileSync(path, 'utf8').replaceAll('\r\n', '\n')).digest('hex')]))
const sources = identity()
const args = ['run', '--locked', '--release', '-p', 'starroom-detail', '--example', 'cancellation_probe']
const result = spawnSync('cargo', args, { encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 })
const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`
process.stdout.write(output)
let measured = null
try { measured = JSON.parse(result.stdout.trim()) } catch { /* Failure is retained, not hidden. */ }
const failure = result.error?.message ?? (result.status !== 0 ? `cargo exit ${result.status}`
  : JSON.stringify(sources) !== JSON.stringify(identity()) ? 'source changed during measurement'
    : !measured?.bitExact || measured.changedSamples !== 5 ? 'missing exact kernel measurements' : null)
const report = { generatedAt: new Date().toISOString(), compiler,
  revision: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
  dirty: execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim() !== '', sources,
  scope: 'Real NR kernel on deterministic unbounded float pixels; excludes UI/IPC/GPU/whole graph/RSS',
  measured, failure, output }
mkdirSync('.starroom-reports', { recursive: true })
const path = `.starroom-reports/detail-cancellation-${Date.now()}.json`
writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`)
console.log(`Report: ${path}`)
if (failure) process.exitCode = 1
