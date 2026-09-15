import { readdirSync, readFileSync } from 'node:fs'

for (const name of readdirSync(new URL('../schemas/', import.meta.url)).filter((name) => name.endsWith('.json'))) {
  JSON.parse(readFileSync(new URL(`../schemas/${name}`, import.meta.url), 'utf8'))
  console.log(`OK schemas/${name}`)
}
