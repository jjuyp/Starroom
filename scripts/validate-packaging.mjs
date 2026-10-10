import { existsSync, readFileSync } from 'node:fs'

const packageJson = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
const tauri = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'))

if (tauri.productName !== 'Starroom' || tauri.version !== packageJson.version) {
  throw new Error('Tauri package identity/version does not match package.json')
}
if (tauri.build?.frontendDist !== '../dist' || tauri.bundle?.active !== true) {
  throw new Error('Tauri bundle or frontendDist configuration is disabled')
}
if (!existsSync(new URL('../dist/index.html', import.meta.url))) {
  throw new Error('Production frontend artifact dist/index.html is missing')
}
console.log(`OK Starroom ${tauri.version} packaging configuration and frontend artifact`)
