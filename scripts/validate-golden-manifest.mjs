import { existsSync, readFileSync, statSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { goldenTags } from './test-target-config.mjs'
import { selectGoldenFixtures } from './select-golden-fixtures.mjs'

const manifest = JSON.parse(readFileSync(new URL('../fixtures/golden/manifest.json', import.meta.url), 'utf8'))
const required = new Set([
  'portrait-daylight', 'portrait-low-key', 'backlit-portrait', 'high-dynamic-range',
  'night-city', 'neon', 'high-iso', 'white-black-clothing', 'colorchecker',
  'fine-texture', 'mixed-temperature',
])
const assertions = ['identity', 'extremeControl', 'finite', 'toneRegression', 'colorRegression']

if (manifest.schemaVersion !== 3 || manifest.workingSpace !== 'linear Rec.2020 D65') {
  throw new Error('Golden manifest schema or working space is invalid')
}
if (JSON.stringify(manifest.supportedTags) !== JSON.stringify(goldenTags)) {
  throw new Error('Golden manifest supportedTags does not match the canonical tag registry')
}
for (const assertion of assertions) {
  if (!manifest.commonAssertions?.includes(assertion)) throw new Error(`Missing common assertion: ${assertion}`)
}
if (!manifest.futureAssertions?.includes('cpuGpuParity')) throw new Error('Missing future CPU/GPU parity contract')
const assets = new Map()
for (const asset of manifest.assets ?? []) {
  for (const field of [
    'id', 'path', 'sourcePageUrl', 'sourceUrl', 'upstream', 'upstreamRevision', 'author',
    'license', 'licenseUrl', 'cameraModel', 'format', 'sha256', 'byteLength', 'width', 'height',
    'bitDepth',
  ]) {
    if (asset[field] === undefined || asset[field] === '') throw new Error(`Golden asset ${asset.id ?? '<unknown>'} lacks ${field}`)
  }
  if (assets.has(asset.id)) throw new Error(`Duplicate Golden asset: ${asset.id}`)
  if (!['CC0-1.0', 'CC-BY-SA-3.0', 'Public-Domain-US-Gov'].includes(asset.license)) {
    throw new Error(`Golden asset has unreviewed license: ${asset.id} (${asset.license})`)
  }
  const sourceUrl = new URL(asset.path, new URL('../fixtures/golden/manifest.json', import.meta.url))
  if (!existsSync(sourceUrl)) throw new Error(`Golden asset is missing: ${asset.path}`)
  const bytes = readFileSync(sourceUrl)
  if (statSync(sourceUrl).size !== asset.byteLength) throw new Error(`Golden asset length mismatch: ${asset.id}`)
  const hash = createHash('sha256').update(bytes).digest('hex')
  if (hash !== asset.sha256) throw new Error(`Golden asset hash mismatch: ${asset.id}`)
  if (asset.licenseFile && !existsSync(new URL(asset.licenseFile, new URL('../fixtures/golden/manifest.json', import.meta.url)))) {
    throw new Error(`Golden asset license text is missing: ${asset.id}`)
  }
  assets.set(asset.id, asset)
}
if (assets.size < 5) throw new Error('Golden photographic corpus must contain at least five reviewed assets')

for (const entry of manifest.cases ?? []) {
  if (!entry.id || !entry.scene || !['planned', 'active'].includes(entry.status)) throw new Error(`Invalid case: ${JSON.stringify(entry)}`)
  if (!entry.extremeControls?.length || !entry.rois?.length) throw new Error(`Case lacks controls or ROIs: ${entry.id}`)
  if (!entry.tags?.length || entry.tags.some((tag) => !goldenTags.includes(tag))) throw new Error(`Case has invalid tags: ${entry.id}`)
  if (!['photographic', 'numerical'].includes(entry.fixtureKind)) throw new Error(`Case lacks fixture kind: ${entry.id}`)
  if (entry.status === 'active' && entry.fixtureKind === 'photographic' && !assets.has(entry.assetId)) {
    throw new Error(`Active photographic case lacks a reviewed asset: ${entry.id}`)
  }
  required.delete(entry.id)
}
if (required.size) throw new Error(`Missing required Golden cases: ${[...required].join(', ')}`)
const requestedTags = (process.argv.find((arg) => arg.startsWith('--tags='))?.slice(7) ?? '')
  .split(',').map((tag) => tag.trim()).filter(Boolean)
const selectedCases = selectGoldenFixtures(manifest, requestedTags)
const colorchecker = manifest.cases.find((entry) => entry.id === 'colorchecker')
if (colorchecker?.referenceOracle?.status !== 'active' || colorchecker.referenceOracle.license !== 'BSD-3-Clause') {
  throw new Error('ColorChecker reference oracle is missing or has an unexpected license')
}
const colorcheckerUrl = new URL(colorchecker.referenceOracle.path, new URL('../fixtures/golden/manifest.json', import.meta.url))
const colorcheckerFixture = JSON.parse(readFileSync(colorcheckerUrl, 'utf8'))
if (colorcheckerFixture.license !== 'BSD-3-Clause'
  || colorcheckerFixture.patches?.length !== colorchecker.referenceOracle.patches
  || !existsSync(new URL(colorcheckerFixture.licenseFile, colorcheckerUrl))) {
  throw new Error('ColorChecker oracle data or retained BSD-3-Clause license is invalid')
}
console.log(`OK fixtures/golden/manifest.json (${selectedCases.length}/${manifest.cases.length} selected cases)`)
console.log(`OK fixtures/golden/sources (${assets.size} immutable photographic assets)`)

if (!requestedTags.length || requestedTags.includes('raw')) {
const rawManifestUrl = new URL('../fixtures/raw/manifest.json', import.meta.url)
const rawManifest = JSON.parse(readFileSync(rawManifestUrl, 'utf8'))
const requiredFormats = new Set(['NEF', 'ARW', 'CR2', 'CR3', 'DNG', 'RAF'])
if (rawManifest.schemaVersion !== 1 || rawManifest.provider?.license !== 'CC0-1.0') {
  throw new Error('RAW fixture manifest schema or provider license is invalid')
}
for (const fixture of rawManifest.fixtures ?? []) {
  for (const field of ['id', 'path', 'upstreamPath', 'sourceUrl', 'cameraMake', 'cameraModel', 'format', 'sha256', 'license']) {
    if (!fixture[field]) throw new Error(`RAW fixture ${fixture.id ?? '<unknown>'} lacks ${field}`)
  }
  if (fixture.license !== 'CC0-1.0') throw new Error(`RAW fixture ${fixture.id} is not CC0`)
  const sourceUrl = new URL(fixture.path, rawManifestUrl)
  if (!existsSync(sourceUrl)) throw new Error(`RAW fixture is missing: ${fixture.path}`)
  const bytes = readFileSync(sourceUrl)
  if (statSync(sourceUrl).size !== fixture.byteLength) throw new Error(`RAW fixture length mismatch: ${fixture.id}`)
  const hash = createHash('sha256').update(bytes).digest('hex')
  if (hash !== fixture.sha256) throw new Error(`RAW fixture hash mismatch: ${fixture.id}`)
  requiredFormats.delete(fixture.format)
}
if (requiredFormats.size) throw new Error(`Missing RAW formats: ${[...requiredFormats].join(', ')}`)
console.log(`OK fixtures/raw/manifest.json (${rawManifest.fixtures.length} CC0 sensor fixtures)`)
}
