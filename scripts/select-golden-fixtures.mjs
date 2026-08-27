import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { goldenTags } from './test-target-config.mjs'

export function selectGoldenFixtures(manifest, requestedTags = [], match = 'any') {
  const unknown = requestedTags.filter((tag) => !goldenTags.includes(tag))
  if (unknown.length) throw new Error(`Unknown Golden tags: ${unknown.join(', ')}`)
  if (!requestedTags.length) return manifest.cases
  return manifest.cases.filter((fixture) => {
    const tags = new Set(fixture.tags ?? [])
    return match === 'all'
      ? requestedTags.every((tag) => tags.has(tag))
      : requestedTags.some((tag) => tags.has(tag))
  })
}

function main() {
  const args = process.argv.slice(2)
  const match = args.includes('--all-tags') ? 'all' : 'any'
  const activeOnly = args.includes('--active-only')
  const tagsArg = args.find((arg) => arg.startsWith('--tags='))?.slice(7) ?? ''
  const tags = tagsArg.split(',').map((tag) => tag.trim()).filter(Boolean)
  const manifest = JSON.parse(readFileSync(new URL('../fixtures/golden/manifest.json', import.meta.url), 'utf8'))
  let fixtures = selectGoldenFixtures(manifest, tags, match)
  if (activeOnly) fixtures = fixtures.filter((fixture) => fixture.status === 'active')
  const result = {
    mode: tags.length ? match : 'full',
    requestedTags: tags,
    selectedCount: fixtures.length,
    fixtures: fixtures.map(({ id, status, tags: fixtureTags }) => ({ id, status, tags: fixtureTags })),
  }
  console.log(JSON.stringify(result, null, 2))
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main()
