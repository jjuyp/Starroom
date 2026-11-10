import { createHash } from 'node:crypto'
import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { WindowsUiWebDriver, webViewDebuggerIsReady } from './windows-ui-webdriver.mjs'
import { runWindowsUiWorkflow, waitForUi } from './windows-ui-workflow.mjs'

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
  TAURI_WEBVIEW_AUTOMATION: 'true',
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
let application
let report
let failure
let debuggerProbe
const browserPolicy = (action) => {
  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive',
    '-File', resolve('scripts/windows-ui-browser-policy.ps1'), '-Action', action, '-OutputRoot', root], {
    env, windowsHide: true, encoding: 'utf8', timeout: 30_000,
  })
  if (result.error || result.status !== 0) {
    throw new Error(`UiGateSetupInvalid: browser policy ${action} failed: ${result.error ?? result.stderr}`)
  }
}
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
  browserPolicy('Enable')
  // Launch owns the explicit fresh WebView profile. Attach avoids EdgeDriver's
  // temporary DevToolsActivePort directory conflicting with that SDK override.
  // The isolated CI window must paint and receive real input; the top-level
  // guard forbids this path on a user desktop. The driver console stays hidden.
  application = spawn(executable, [], { env: {
    ...env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
      '--remote-debugging-port=9223 --remote-debugging-address=127.0.0.1',
  }, windowsHide: false, stdio: ['ignore', 'pipe', 'pipe'] })
  let applicationError
  application.once('error', (error) => { applicationError = error })
  for (const stream of [application.stdout, application.stderr]) {
    stream.on('data', (data) => { diagnostic = (diagnostic + data.toString()).slice(-1024 * 1024) })
  }
  let debuggerReady = false
  for (let attempt = 0; attempt < 120; attempt++) {
    if (applicationError || application.exitCode !== null) {
      throw new Error(`UiGateApplicationExited: production executable exited before WebView2 initialization (code=${application.exitCode}, error=${applicationError ?? 'none'})`)
    }
    try {
      const response = await fetch('http://127.0.0.1:9223/json/version', { signal: AbortSignal.timeout(1000) })
      const info = await response.json()
      debuggerProbe = { status: response.status, info }
      debuggerReady = response.ok && webViewDebuggerIsReady(info)
      if (debuggerReady) break
    } catch (error) {
      debuggerProbe = { error: String(error), cause: String(error.cause ?? '') }
    }
    await delay(250)
  }
  if (!debuggerReady) throw new Error('UiGateSetupUnavailable: owned WebView2 debugger did not initialize')
  await driver.attach('127.0.0.1:9223')
  await waitForUi(driver,
    "return location.hostname==='tauri.localhost' || location.protocol==='tauri:'", [], 30_000)
  report = await runWindowsUiWorkflow(driver, fixture)
  if (sha256(fixture) !== originalHash) throw new Error('UiGateSourceMutated: public fixture changed')
  const screenshot = await driver.command(`/session/${driver.session}/screenshot`, undefined, 'GET')
  if (typeof screenshot !== 'string') throw new Error('UiGateCaptureFailed: actual WebView2 screenshot missing')
  writeFileSync(join(root, 'success.png'), Buffer.from(screenshot, 'base64'))
  writeFileSync(join(root, 'ui-qualification.json'), JSON.stringify({
    schemaVersion: 1, generatedAt: new Date().toISOString(), revision: process.env.GITHUB_SHA,
    executableSha256: sha256(executable), fixtureSha256: originalHash,
    driver: identity, ...report, screenshot: 'success.png', debuggerProbe,
  }, null, 2) + '\n')
  console.log('STARROOM_WINDOWS_UI ' + JSON.stringify(report))
} catch (error) {
  failure = error
  const details = { scope: 'Isolated CI public fixture only', message: String(error) }
  details.debuggerProbe = debuggerProbe ?? null
  if (application?.pid) {
    const snapshot = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive',
      '-File', resolve('scripts/capture-windows-ui-startup.ps1'),
      '-ApplicationId', String(application.pid), '-OutputRoot', root], {
      env, windowsHide: true, encoding: 'utf8', timeout: 30_000,
    })
    details.startupCaptureExitCode = snapshot.status
    details.startupCaptureDiagnostic = (snapshot.stderr ?? '').slice(-2000)
  }
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
    application?.kill()
    child.kill()
    try { browserPolicy('Disable') } catch (cleanupError) {
      diagnostic += '\nPolicy cleanup: ' + String(cleanupError)
      if (!failure) throw cleanupError
    } finally {
      writeFileSync(join(root, 'driver.log'), diagnostic)
    }
  }
}
