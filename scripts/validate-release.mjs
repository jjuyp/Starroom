import { existsSync, readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'

const root = new URL('../', import.meta.url)
const read = (path) => readFileSync(new URL(path, root), 'utf8')
const pkg = JSON.parse(read('package.json'))
const tauri = JSON.parse(read('src-tauri/tauri.conf.json'))
const cargo = read('Cargo.toml')
const cargoLock = read('Cargo.lock')
const expectedArg = process.argv.find((arg) => arg.startsWith('--expected-version='))
const expected = expectedArg?.split('=', 2)[1] ?? pkg.version

if (pkg.version !== expected || tauri.version !== expected) {
  throw new Error(`Release versions differ: expected=${expected} package=${pkg.version} tauri=${tauri.version}`)
}
if (!cargo.includes(`version = "${expected}"`)) throw new Error('Cargo workspace version differs from the release version')
const starroomLockVersions = [...cargoLock.matchAll(/\[\[package\]\]\r?\nname = "(starroom-[^"]+)"\r?\nversion = "([^"]+)"/g)]
if (!starroomLockVersions.length || starroomLockVersions.some(([, , version]) => version !== expected)) {
  throw new Error(`Cargo.lock contains a stale Starroom package version; expected ${expected}`)
}
if (!tauri.bundle?.active || !tauri.bundle?.icon?.includes('icons/icon.ico')) throw new Error('Windows bundle or release icon is not configured')
if (tauri.bundle?.licenseFile !== '../LICENSE') throw new Error('Windows bundle license file is not configured')

for (const path of [
  'LICENSE',
  'THIRD_PARTY_NOTICES.md',
  'THIRD_PARTY_LICENSES.txt',
  'NOTICE.md',
  'MODEL_PROVENANCE.md',
  'docs/17_THIRD_PARTY_PROVENANCE.md',
  'docs/36_M30_DEPENDENCY_LICENSE_REPORT.json',
  'src-tauri/icons/icon.ico',
  'vendor/libraw-0.22.2/LICENSE.CDDL',
]) {
  if (!existsSync(new URL(path, root))) throw new Error(`Missing release notice or asset: ${path}`)
}

const bundledResources = tauri.bundle?.resources ?? {}
for (const destination of [
  'LICENSE',
  'THIRD_PARTY_NOTICES.md',
  'THIRD_PARTY_LICENSES.txt',
  'NOTICE.md',
  'MODEL_PROVENANCE.md',
  'docs/17_THIRD_PARTY_PROVENANCE.md',
  'docs/36_M30_DEPENDENCY_LICENSE_REPORT.json',
]) {
  if (!Object.values(bundledResources).includes(destination)) {
    throw new Error(`Required notice is not bundled: ${destination}`)
  }
}

const tracked = spawnSync('git', ['ls-files'], { cwd: new URL('.', root), encoding: 'utf8' })
if (tracked.status !== 0) throw new Error(tracked.stderr || 'git ls-files failed')
const trackedModels = tracked.stdout.split(/\r?\n/).filter((path) => /(^|\/)models\/local\/|\.(onnx|pth|pt)$/i.test(path))
if (trackedModels.length) throw new Error(`Local/non-redistributable model weights are tracked: ${trackedModels.join(', ')}`)

const productionPaths = tracked.stdout.split(/\r?\n/).filter((path) =>
  /^(src|src-tauri\/src|crates\/[^/]+\/src)\//.test(path) && /\.(rs|ts|tsx|js|mjs)$/.test(path))
const networkFindings = []
for (const path of productionPaths) {
  const source = read(path)
  if (/\b(fetch|WebSocket|XMLHttpRequest)\s*\(|\b(reqwest|ureq|TcpStream|UdpSocket|tokio::net|std::net)::/.test(source)) {
    networkFindings.push(path)
  }
}
if (networkFindings.length) throw new Error(`Unexpected production network surface: ${networkFindings.join(', ')}`)

console.log(`OK Starroom ${expected} release identity (${starroomLockVersions.length} locked packages), local-model exclusion and offline production-source scan`)
