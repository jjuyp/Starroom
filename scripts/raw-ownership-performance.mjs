import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

// Invoke an already-built Release probe once per process. Only the six pinned,
// publicly redistributable RAW fixtures are admitted; no personal source paths.
const digest = (path) => createHash('sha256').update(readFileSync(path)).digest('hex')
const manifestPath = 'fixtures/raw/manifest.json'
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
const probe = resolve('target/release/examples/decode_ownership_probe' + (process.platform === 'win32' ? '.exe' : ''))
const referenceArg = process.argv.find((arg) => arg.startsWith('--reference='))
const reference = referenceArg ? JSON.parse(readFileSync(referenceArg.slice(12), 'utf8')) : null
const compiler = execFileSync('rustc', ['-Vv'], { encoding: 'utf8', windowsHide: true }).trim()
const probeSource = 'crates/starroom-raw/examples/decode_ownership_probe.rs'
const inputs = Object.fromEntries([manifestPath, probeSource, 'crates/starroom-raw/src/lib.rs',
  'crates/starroom-raw/src/libraw_bridge.cpp', probe].map((path) => [path, digest(path)]))
if (reference && (reference.compiler !== compiler || reference.manifestSha256 !== digest(manifestPath)
  || reference.probeSourceSha256 !== digest(probeSource))) {
  throw new Error('OwnershipProbeInvalid: reference compiler, manifest or probe source differs')
}
const cases = []
for (const fixture of manifest.fixtures) {
  const path = resolve('fixtures/raw', fixture.path)
  if (fixture.license !== 'CC0-1.0' || digest(path) !== fixture.sha256) {
    throw new Error('OwnershipProbeInvalid: public fixture license or SHA does not match')
  }
  for (const mode of ['full', 'half']) {
    const referenceCase = reference?.cases.find((entry) => entry.id === fixture.id && entry.mode === mode)
    if (reference && (!referenceCase || referenceCase.runs.length !== 3)) {
      throw new Error('OwnershipProbeInvalid: complete reference case missing')
    }
    const runs = []
    for (let round = 0; round < 3; round++) {
      const value = JSON.parse(execFileSync(probe, [path, mode], {
        encoding: 'utf8', windowsHide: true, timeout: 180_000,
      }))
      if (!value.sourceImmutable || value.sourceSha256 !== fixture.sha256) {
        throw new Error('OwnershipProbeFailed: source identity changed')
      }
      const first = runs[0]
      for (const expected of [first, referenceCase?.runs[round]]) {
        if (!expected) continue
        for (const key of ['width', 'height', 'sampleCount', 'rgbF32BitsSha256', 'metadataSha256']) {
          if (value[key] !== expected[key]) throw new Error(`OwnershipProbeFailed: ${fixture.id}/${mode}/${key} changed`)
        }
      }
      runs.push(value)
    }
    const median = (values) => [...values].sort((a, b) => a - b)[1]
    cases.push({ id: fixture.id, mode, runs,
      medianDecodeWallMs: median(runs.map((run) => run.decodeWallMs)),
      medianPeakBytes: runs.every((run) => Number.isFinite(run.peakWorkingSetBeforeProbeValidationBytes))
        ? median(runs.map((run) => run.peakWorkingSetBeforeProbeValidationBytes)) : null,
    })
    console.log(`RAW ownership ${fixture.id}/${mode}: exact deterministic f32/metadata, three isolated processes`)
  }
}
for (const [path, hash] of Object.entries(inputs)) {
  if (digest(path) !== hash) throw new Error('OwnershipProbeInvalid: implementation or binary changed during measurement')
}
const report = {
  schemaVersion: 1, generatedAt: new Date().toISOString(), compiler,
  revision: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8', windowsHide: true }).trim(),
  dirty: !!execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8', windowsHide: true }).trim(),
  manifestSha256: digest(manifestPath), probeSourceSha256: digest(probeSource),
  bridgeSha256: digest('crates/starroom-raw/src/libraw_bridge.cpp'),
  rustAdapterSha256: digest('crates/starroom-raw/src/lib.rs'),
  executableSha256: digest(probe), exactReferenceMatched: reference ? true : null,
  scope: 'Same-compiler decode ownership equivalence and process peak at completed decode; not photographic quality or UI latency',
  comparisons: reference ? cases.map((entry) => {
    const before = reference.cases.find((old) => old.id === entry.id && old.mode === entry.mode)
    return { id: entry.id, mode: entry.mode, beforeDecodeMs: before.medianDecodeWallMs,
      afterDecodeMs: entry.medianDecodeWallMs, beforePeakBytes: before.medianPeakBytes,
      afterPeakBytes: entry.medianPeakBytes }
  }) : null,
  cases,
}
mkdirSync('.starroom-reports', { recursive: true })
const reportPath = `.starroom-reports/raw-ownership-${Date.now()}.json`
writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n')
console.log('Ownership report: ' + reportPath)
