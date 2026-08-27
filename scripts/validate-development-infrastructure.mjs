import { readFileSync } from 'node:fs'
import { classifyPaths } from './ci-changed-paths.mjs'
import { goldenTags, targets } from './test-target-config.mjs'
import { selectGoldenFixtures } from './select-golden-fixtures.mjs'

const manifest = JSON.parse(readFileSync(new URL('../fixtures/golden/manifest.json', import.meta.url), 'utf8'))

for (const [name, target] of Object.entries(targets)) {
  for (const tag of target.golden) {
    if (!goldenTags.includes(tag)) throw new Error(`${name} references unknown Golden tag ${tag}`)
  }
}

const m7 = selectGoldenFixtures(manifest, ['color', 'portrait', 'skin', 'neon', 'landscape'])
if (!m7.some(({ id }) => id === 'portrait-daylight') || !m7.some(({ id }) => id === 'neon')) {
  throw new Error('M7 Golden tag union does not include its required fixture families')
}
const colorOnly = classifyPaths(['crates/starroom-color/src/lib.rs'])
if (!colorOnly.color || colorOnly.raw || colorOnly.geometry || colorOnly.ai) {
  throw new Error('Color-only path classification has an unrelated CI fan-out')
}
const shared = classifyPaths(['crates/starroom-pipeline/src/lib.rs'])
if (Object.values(shared).some((enabled) => !enabled)) throw new Error('Shared graph changes must broaden all targeted checks')

console.log(`OK acceleration infrastructure (${Object.keys(targets).length} targets, ${goldenTags.length} tags)`)
