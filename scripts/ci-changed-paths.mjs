import { execFileSync } from 'node:child_process'
import { appendFileSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const categories = ['web', 'library', 'history', 'export', 'color', 'raw', 'detail', 'optics', 'geometry', 'gpu', 'ai']
const rustLabels = {
  library: 'Library Check', history: 'History Check', export: 'Export Check',
  color: 'Color Check', raw: 'RAW Check', detail: 'Detail Check', optics: 'Optics Check',
  geometry: 'Geometry Check', ai: 'AI Check',
  gpu: 'GPU Check',
}

const rules = {
  web: [/^src\//, /^src-tauri\//, /^package(?:-lock)?\.json$/, /^vite\.config\./, /^tsconfig/, /^eslint\.config\./],
  library: [/^crates\/starroom-library\//],
  history: [/^crates\/starroom-history\//],
  export: [/^crates\/starroom-export\//],
  color: [/^crates\/starroom-(?:color|color-management|grading|reference|look)\//],
  raw: [/^crates\/starroom-(?:raw|imageio)\//, /^fixtures\/raw\//, /^fixtures\/colorchecker\//],
  detail: [/^crates\/starroom-(?:detail|heal|portrait|ai-denoise|look)\//],
  optics: [/^crates\/starroom-optics\//],
  geometry: [/^crates\/starroom-geometry\//],
  gpu: [/^crates\/starroom-render\//],
  ai: [/^crates\/starroom-(?:advisor|ai-denoise|reference|look)\//, /^models\//],
}

const broadRules = [
  /^Cargo\.toml$/, /^Cargo\.lock$/, /^crates\/starroom-core\//,
  /^crates\/starroom-(?:pipeline|render|project)\//,
  /^\.github\/workflows\//, /^scripts\/(?:test-target|test-target-config|ci-changed-paths|validate-development-infrastructure)\.mjs$/,
]

export function classifyPaths(paths) {
  const normalized = paths.map((path) => path.replaceAll('\\', '/'))
  const broad = normalized.some((path) => broadRules.some((rule) => rule.test(path)))
  return Object.fromEntries(categories.map((category) => [
    category,
    broad || normalized.some((path) => rules[category].some((rule) => rule.test(path))),
  ]))
}

function changedPaths() {
  if (process.argv.includes('--stdin')) {
    return readFileSync(0, 'utf8').split(/\r?\n/).filter(Boolean)
  }
  const before = process.env.STARROOM_BASE_SHA
  const head = process.env.STARROOM_HEAD_SHA || 'HEAD'
  const validBefore = before && !/^0+$/.test(before)
  const base = validBefore ? before : `${head}^`
  try {
    return execFileSync('git', ['diff', '--name-only', base, head], { encoding: 'utf8' }).split(/\r?\n/).filter(Boolean)
  } catch {
    return execFileSync('git', ['show', '--pretty=', '--name-only', head], { encoding: 'utf8' }).split(/\r?\n/).filter(Boolean)
  }
}

function main() {
  const paths = changedPaths()
  const flags = classifyPaths(paths)
  const eventName = process.env.GITHUB_EVENT_NAME ?? 'local'
  const forced = process.env.STARROOM_FORCE_FULL === 'true'
  const releaseTag = (process.env.GITHUB_REF ?? '').startsWith('refs/tags/')
  let acceptanceCommit = false
  try {
    const commit = process.env.STARROOM_HEAD_SHA || 'HEAD'
    acceptanceCommit = execFileSync('git', ['log', '-1', '--pretty=%B', commit], { encoding: 'utf8' }).includes('[full-acceptance]')
  } catch {
    // A shallow/non-git validation context simply cannot opt into Full Acceptance.
  }
  const full = forced || releaseTag || (eventName === 'push' && acceptanceCommit)
  // The push check is attached to the same commit in the PR. Avoid duplicating its
  // authoritative Full Acceptance with a second PR-targeted fan-out.
  if (eventName === 'pull_request' && acceptanceCommit) {
    for (const category of categories) flags[category] = false
  }
  const rustTargets = Object.keys(rustLabels).filter((target) => flags[target])
  const rustMatrix = { include: rustTargets.map((target) => ({ target, label: rustLabels[target] })) }
  const output = { paths, ...flags, full, rustAny: rustTargets.length > 0, rustMatrix }
  console.log(JSON.stringify(output, null, 2))
  if (process.env.GITHUB_OUTPUT) {
    for (const [key, value] of Object.entries({ ...flags, full, rust_any: rustTargets.length > 0 })) {
      appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${value}\n`)
    }
    appendFileSync(process.env.GITHUB_OUTPUT, `rust_matrix=${JSON.stringify(rustMatrix)}\n`)
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main()
