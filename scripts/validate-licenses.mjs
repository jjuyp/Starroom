import { createHash } from 'node:crypto'
import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = new URL('../', import.meta.url)
const rootPath = fileURLToPath(root)
const reportUrl = new URL('../docs/36_M30_DEPENDENCY_LICENSE_REPORT.json', import.meta.url)
const textsUrl = new URL('../THIRD_PARTY_LICENSES.txt', import.meta.url)
const cargoLock = readFileSync(new URL('Cargo.lock', root))
const npmLock = readFileSync(new URL('package-lock.json', root))
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')
const canonicalText = (bytes) => Buffer.from(bytes.toString('utf8').replace(/\r\n/g, '\n'))
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
const rustPackages = JSON.parse(metadata.stdout).packages.filter((pkg) => pkg.source)
const rust = rustPackages
  .map((pkg) => {
    assertLicense(`Rust ${pkg.name}@${pkg.version}`, pkg.license)
    return { name: pkg.name, version: pkg.version, license: pkg.license, source: pkg.source }
  })
  .sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`))

const packageLock = JSON.parse(npmLock)
const npmPackages = Object.entries(packageLock.packages).filter(([path, pkg]) => path && pkg.dev !== true)
const npm = npmPackages
  .map(([path, pkg]) => {
    const name = path.replace(/^node_modules\//, '')
    assertLicense(`npm ${name}@${pkg.version}`, pkg.license)
    return { name, version: pkg.version, license: pkg.license, integrity: pkg.integrity }
  })
  .sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`))

const report = {
  schemaVersion: 1,
  policy: 'GPL-3.0-or-later combined work; preserve file-level and notice obligations',
  cargoLockSha256: sha256(canonicalText(cargoLock)),
  packageLockSha256: sha256(canonicalText(npmLock)),
  rust,
  npm,
}
const serialized = `${JSON.stringify(report, null, 2)}\n`

const licenseDocuments = new Map()
const missingDocuments = []
function collectLicenseDocuments(owner, directory) {
  const files = readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /^(LICENSE|LICENCE|COPYING|UNLICENSE|NOTICE)([._-].*)?$/i.test(entry.name))
    .map((entry) => entry.name)
    .sort()
  if (!files.length) missingDocuments.push(owner)
  for (const file of files) {
    const content = readFileSync(join(directory, file), 'utf8').replace(/\r\n/g, '\n').trimEnd()
    const hash = sha256(Buffer.from(content))
    const document = licenseDocuments.get(hash) ?? { hash, content, owners: [], files: [] }
    document.owners.push(owner)
    document.files.push(file)
    licenseDocuments.set(hash, document)
  }
}
for (const pkg of rustPackages) {
  collectLicenseDocuments(`Rust ${pkg.name}@${pkg.version}`, dirname(pkg.manifest_path))
}
for (const [path, pkg] of npmPackages) {
  collectLicenseDocuments(`npm ${path.replace(/^node_modules\//, '')}@${pkg.version}`, join(rootPath, path))
}
const licenseTexts = [
  'STARROOM THIRD-PARTY LICENSE TEXTS',
  '',
  'Generated deterministically from Cargo.lock and package-lock.json package contents.',
  'Packages without a standalone top-level license document remain identified by SPDX expression',
  'in docs/36_M30_DEPENDENCY_LICENSE_REPORT.json.',
  '',
  `Packages without standalone document: ${missingDocuments.sort().join(', ') || '(none)'}`,
  '',
  ...[...licenseDocuments.values()]
    .sort((a, b) => a.hash.localeCompare(b.hash))
    .flatMap((document) => [
      '='.repeat(80),
      `SHA-256: ${document.hash}`,
      `Packages: ${[...new Set(document.owners)].sort().join(', ')}`,
      `Source filenames: ${[...new Set(document.files)].sort().join(', ')}`,
      '='.repeat(80),
      document.content,
      '',
    ]),
].join('\n')

if (process.argv.includes('--write')) {
  writeFileSync(reportUrl, serialized)
  writeFileSync(textsUrl, licenseTexts)
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
  if (readFileSync(textsUrl, 'utf8').replace(/\r\n/g, '\n') !== licenseTexts) {
    throw new Error('Bundled third-party license texts are stale')
  }
}

console.log(`OK dependency licenses: ${rust.length} Rust packages, ${npm.length} npm production packages, ${licenseDocuments.size} unique license/notice texts`)
