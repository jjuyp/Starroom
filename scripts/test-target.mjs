import { mkdirSync, writeFileSync } from 'node:fs'
import { performance } from 'node:perf_hooks'
import { spawnSync } from 'node:child_process'
import { targets, sharedGraphRust } from './test-target-config.mjs'
import { selectGoldenFixtures } from './select-golden-fixtures.mjs'
import goldenManifest from '../fixtures/golden/manifest.json' with { type: 'json' }

const args = process.argv.slice(2)
const levelIndex = args.indexOf('--level')
const level = levelIndex >= 0 ? args[levelIndex + 1] : 'targeted'
const rustOnly = args.includes('--rust-only')
const webOnly = args.includes('--web-only')
if (rustOnly && webOnly) throw new Error('--rust-only and --web-only are mutually exclusive')
const targetName = args.find((arg) => !arg.startsWith('-') && arg !== level) ?? (level === 'full' ? 'full' : process.env.STARROOM_TEST_TARGET)
const target = targets[targetName]

if (level !== 'full' && !target) {
  throw new Error(`Choose a target: ${Object.keys(targets).join(', ')}`)
}

const records = []
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm'

function run(label, command, commandArgs) {
  const started = performance.now()
  console.log(`\n== ${label}: ${command} ${commandArgs.join(' ')}`)
  const result = spawnSync(command, commandArgs, {
    stdio: 'inherit',
    shell: process.platform === 'win32' && command.endsWith('.cmd'),
    env: process.env,
  })
  const durationSeconds = Number(((performance.now() - started) / 1000).toFixed(3))
  records.push({ label, command: [command, ...commandArgs], durationSeconds, exitCode: result.status ?? 1 })
  if (result.error) throw result.error
  if (result.status !== 0) finish(result.status ?? 1)
}

function golden(label, tags) {
  const started = performance.now()
  const fixtures = selectGoldenFixtures(goldenManifest, tags)
  const selectionDuration = Number(((performance.now() - started) / 1000).toFixed(3))
  console.log(`\n== ${label}: ${fixtures.map((fixture) => fixture.id).join(', ') || '(no fixtures)'}`)
  records.push({
    label: `${label} selection`,
    tags,
    fixtureCount: fixtures.length,
    durationSeconds: selectionDuration,
    exitCode: 0,
  })
  const validatorArgs = ['scripts/validate-golden-manifest.mjs']
  if (tags.length) validatorArgs.push(`--tags=${tags.join(',')}`)
  run(`${label} manifest validation`, 'node', validatorArgs)
}

function finish(exitCode = 0) {
  const directory = '.starroom-reports'
  mkdirSync(directory, { recursive: true })
  const report = {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    target: targetName ?? 'full',
    level,
    platform: `${process.platform}-${process.arch}`,
    totalDurationSeconds: Number(records.reduce((sum, record) => sum + record.durationSeconds, 0).toFixed(3)),
    records,
  }
  const path = `${directory}/test-timing-${Date.now()}.json`
  writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`\nTiming report: ${path}`)
  process.exit(exitCode)
}

if (level === 'full') {
  if (!rustOnly) {
    run('JSON validation', 'node', ['scripts/validate-json.mjs'])
    run('Golden full validation', 'node', ['scripts/validate-golden-manifest.mjs'])
    run('Frontend lint', npmCommand, ['run', 'lint'])
    run('Vitest full', npmCommand, ['test'])
    run('Frontend production build', npmCommand, ['run', 'build'])
    run('Packaging configuration', 'node', ['scripts/validate-packaging.mjs'])
  }
  if (!webOnly) {
    run('Rust format', 'cargo', ['fmt', '--all', '--', '--check'])
    run('Rust clippy', 'cargo', ['clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings'])
    run('Full Rust workspace', 'cargo', ['test', '--locked', '--workspace'])
    if (process.env.STARROOM_PERFORMANCE_GATE === 'true') {
      for (const rustArgs of targets.performance.rust) run('M28 performance corpus', 'cargo', rustArgs)
    }
  }
  finish()
}

if (!webOnly) for (const rustArgs of target.rust) run(`Rust ${targetName}`, 'cargo', rustArgs)
if (!rustOnly && target.web.length) run(`Vitest ${targetName}`, npmCommand, ['exec', 'vitest', 'run', ...target.web])
golden(`Golden ${targetName} subset`, target.golden)

if (level === 'milestone') {
  if (!webOnly) {
    for (const rustArgs of sharedGraphRust) run('Shared graph regression', 'cargo', rustArgs)
    run('Rust format', 'cargo', ['fmt', '--all', '--', '--check'])
    run('Rust clippy', 'cargo', ['clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings'])
  }
  if (!rustOnly) {
    run('Frontend lint', npmCommand, ['run', 'lint'])
    run('Frontend production build', npmCommand, ['run', 'build'])
  }
}
finish()
