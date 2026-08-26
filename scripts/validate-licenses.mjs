import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'

const root = new URL('../', import.meta.url)
const reportUrl = new URL('../docs/36_M30_DEPENDENCY_LICENSE_REPORT.json', import.meta.url)
const cargoLock = readFileSync(new URL('Cargo.lock', root))
const npmLock = readFileSync(new URL('package-lock.json', root))
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')
const allowed = new Set([
  '0BSD', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'CC0-1.0',
  'CDLA-Permissive-2.0', 'ISC', 'LGPL-2.1-or-later', 'MIT', 'MIT-0',
  'MPL-2.0', 'Unicode-3.0', 'Unlicense', 'Zlib',
])
const operators = new Set(['AND', 'OR', 'WITH', 'LLVM-exception'])

function assertLicense(owner, expression) {
  if (typeof expression !== 'string' || !expression.trim()) {
    throw new Error(`${owner} has no declared license`)
  }
  const identifiers = expression.match(/[A-Za-z0-9][A-Za-z0-9.-]*/g) ?? []
  const unknown = identifiers.filter((value) => !allowed.has(value) && !operators.has(value))
  if (unknown.length) throw new Error(`${owner} has unreviewed license tokens: ${unknown.join(', ')}`)
}

const metadata = spawnSync('cargo', ['metadata', '--locked', '--format-version', '1'], {
  cwd: root,
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
})
if (metadata.status !== 0) throw new Error(metadata.stderr || 'cargo metadata failed')
const rust = JSON.parse(metadata.stdout).packages
  .filter((pkg) => pkg.source)
  .map((pkg) => {
    assertLicense(`Rust ${pkg.name}@${pkg.version}`, pkg.license)
    return { name: pkg.name, version: pkg.version, license: pkg.license, source: pkg.source }
  })
  .sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`))

const packageLock = JSON.parse(npmLock)
const npm = Object.entries(packageLock.packages)
  .filter(([path, pkg]) => path && pkg.dev !== true)
  .map(([path, pkg]) => {
    const name = path.replace(/^node_modules\//, '')
    assertLicense(`npm ${name}@${pkg.version}`, pkg.license)
    return { name, version: pkg.version, license: pkg.license, integrity: pkg.integrity }
  })
  .sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`))

const report = {
  schemaVersion: 1,
  policy: 'GPL-3.0-or-later combined work; preserve file-level and notice obligations',
  cargoLockSha256: sha256(cargoLock),
  packageLockSha256: sha256(npmLock),
  rust,
  npm,
}
const serialized = `${JSON.stringify(report, null, 2)}\n`

if (process.argv.includes('--write')) {
  writeFileSync(reportUrl, serialized)
} else {
  const existing = JSON.parse(readFileSync(reportUrl, 'utf8'))
  if (existing.schemaVersion !== report.schemaVersion
      || existing.cargoLockSha256 !== report.cargoLockSha256
      || existing.packageLockSha256 !== report.packageLockSha256) {
    throw new Error('Dependency license report lock identity is stale; run npm run licenses:update and review it')
  }
  const reviewedRust = new Map(existing.rust.map((pkg) => [
    `${pkg.name}@${pkg.version}|${pkg.source}`,
    pkg.license,
  ]))
  for (const pkg of rust) {
    const key = `${pkg.name}@${pkg.version}|${pkg.source}`
    if (reviewedRust.get(key) !== pkg.license) {
      throw new Error(`Rust dependency is absent or changed in the reviewed report: ${key}`)
    }
  }
  if (JSON.stringify(existing.npm) !== JSON.stringify(npm)) {
    throw new Error('Production npm dependency licenses differ from the reviewed report')
  }
}

console.log(`OK dependency licenses: ${rust.length} Rust packages, ${npm.length} npm production packages`)
