import { createHash } from 'node:crypto'
import { spawn } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { WindowsUiWebDriver } from './windows-ui-webdriver.mjs'
import { runWindowsUiWorkflow } from './windows-ui-workflow.mjs'

if (process.platform !== 'win32' || process.env.GITHUB_ACTIONS !== 'true'
  || process.env.RUNNER_OS !== 'Windows' || process.env.STARROOM_WINDOWS_UI_GATE !== '1') {
  throw new Error('UiGateInvalid: explicit isolated Windows CI execution required; never launch a user desktop')
}
const executable = resolve(process.argv.find((arg) => arg.startsWith('--executable='))?.slice(13) ?? '')
if (!executable.endsWith('.exe')) throw new Error('UiGateInvalid: explicit production executable required')
const tool = process.env.STARROOM_EDGE_DRIVER
const identityPath = process.env.STARROOM_EDGE_DRIVER_IDENTITY
if (!tool || !identityPath) throw new Error('UiGateSetupUnavailable: verified EdgeDriver is missing')
const identity = JSON.parse(readFileSync(identityPath, 'utf8').replace(/^\uFEFF/, ''))
const sha256 = (path) => createHash('sha256').update(readFileSync(path)).digest('hex')
if (sha256(tool) !== identity.driverSha256) throw new Error('UiGateSetupInvalid: driver identity changed')
const fixture = resolve('fixtures/golden/sources/astronaut-eileen-collins.png')
const originalHash = sha256(fixture)
mkdirSync('output/qualification', { recursive: true })
const root = mkdtempSync(resolve('output/qualification/windows-ui-'))
const env = {}
for (const [key, value] of Object.entries(process.env)) {
  if (!/TOKEN|SECRET|PASSWORD|CREDENTIAL|API_KEY/i.test(key)) env[key] = value
}
Object.assign(env, {
  APPDATA: join(root, 'roaming'), LOCALAPPDATA: join(root, 'local'),
  WEBVIEW2_USER_DATA_FOLDER: join(root, 'webview'),
  STARROOM_LOCAL_MODELS: join(root, 'private-models-not-bundled'),
})
for (const path of [env.APPDATA, env.LOCALAPPDATA, env.WEBVIEW2_USER_DATA_FOLDER]) {
  mkdirSync(path, { recursive: true })
}
const child = spawn(tool, ['--port=9515', '--allowed-ips=127.0.0.1'], {
  env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
})
let diagnostic = ''
for (const stream of [child.stdout, child.stderr]) {
  stream.on('data', (data) => { diagnostic = (diagnostic + data.toString()).slice(-1024 * 1024) })
}
let launchError
child.once('error', (error) => { launchError = error })
const driver = new WindowsUiWebDriver('http://127.0.0.1:9515/')
let report
let failure
try {
  let ready = false
  for (let attempt = 0; attempt < 120; attempt++) {
    if (launchError || child.exitCode !== null) throw new Error('UiGateSetupUnavailable: driver exited')
    try {
      const response = await fetch('http://127.0.0.1:9515/status', { signal: AbortSignal.timeout(1000) })
      ready = response.ok && (await response.json()).value?.ready
      if (ready) break
    } catch { /* startup only; a live process is still being checked */ }
    await delay(250)
  }
  if (!ready) throw new Error('UiGateSetupUnavailable: driver never became ready')
  await driver.launch(executable)
  report = await runWindowsUiWorkflow(driver, fixture)
  if (sha256(fixture) !== originalHash) throw new Error('UiGateSourceMutated: public fixture changed')
  writeFileSync(join(root, 'ui-qualification.json'), JSON.stringify({
    schemaVersion: 1, generatedAt: new Date().toISOString(), revision: process.env.GITHUB_SHA,
    executableSha256: sha256(executable), fixtureSha256: originalHash,
    driver: identity, ...report,
  }, null, 2) + '\n')
  console.log('STARROOM_WINDOWS_UI ' + JSON.stringify(report))
} catch (error) {
  failure = error
  const details = { scope: 'Isolated CI public fixture only', message: String(error) }
  if (driver.session) {
    try {
      details.visibleText = await driver.execute('return document.body.innerText.slice(0,20000)')
      const png = await driver.command(`/session/${driver.session}/screenshot`, undefined, 'GET')
      if (typeof png === 'string') writeFileSync(join(root, 'failure.png'), Buffer.from(png, 'base64'))
    } catch (diagnosticError) { details.captureError = String(diagnosticError) }
  }
  writeFileSync(join(root, 'ui-failure.json'), JSON.stringify(details, null, 2) + '\n')
  throw error
} finally {
  try { await driver.close() } catch (closeError) {
    diagnostic += '\nCleanup: ' + String(closeError)
    if (!failure) throw closeError
  } finally {
    child.kill()
    writeFileSync(join(root, 'driver.log'), diagnostic)
  }
}
