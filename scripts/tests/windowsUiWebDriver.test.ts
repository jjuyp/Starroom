import { expect, it, vi } from 'vitest'
import { spawnSync } from 'node:child_process'
import { WindowsUiWebDriver, webViewDebuggerIsReady } from '../windows-ui-webdriver.mjs'

it('refuses to launch any desktop application outside an explicitly isolated CI runner', () => {
  const result = spawnSync(process.execPath, ['scripts/windows-ui-qualification.mjs',
    '--executable=must-never-launch.exe'], {
    encoding: 'utf8', env: { ...process.env, GITHUB_ACTIONS: 'false',
      RUNNER_OS: 'Windows', STARROOM_WINDOWS_UI_GATE: '1' },
  })
  expect(result.status).not.toBe(0)
  expect(result.stderr).toContain('never launch a user desktop')
})

it('only permits loopback driver transport and rejects protocol errors', async () => {
  for (const url of ['https://example.org/', 'http://127.0.0.1@evil.test/', 'http://127.0.0.1/api']) {
    expect(() => new WindowsUiWebDriver(url)).toThrow('UiGateInvalid')
  }
  const request = vi.fn(async () => ({ ok: false, status: 500,
    json: async () => ({ value: { error: 'session not created', message: 'runtime mismatch' } }) }))
  const driver = new WindowsUiWebDriver('http://127.0.0.1:9515/', request)
  await expect(driver.launch('test-only.exe')).rejects.toThrow('runtime mismatch')
  expect(driver.session).toBeNull()
})

it('uses production WebView2 capabilities, typed W3C elements, real input and explicit cleanup', async () => {
  const request = vi.fn()
    .mockResolvedValueOnce({ ok: true, json: async () => ({ value: { sessionId: 'owned' } }) })
    .mockResolvedValueOnce({ ok: true, json: async () => ({ value: null }) })
    .mockResolvedValueOnce({ ok: true, json: async () => ({ value: { 'element-6066-11e4-a52e-4f735466cecf': 'button' } }) })
    .mockResolvedValue({ ok: true, json: async () => ({ value: null }) })
  const driver = new WindowsUiWebDriver('http://127.0.0.1:9515/', request)
  await driver.launch('installed.exe')
  expect(JSON.parse(request.mock.calls[0][1].body).capabilities.alwaysMatch)
    .toEqual({ browserName: 'webview2', 'ms:edgeChromium': true, 'ms:edgeOptions': { binary: 'installed.exe' } })
  await driver.click('button')
  expect(request.mock.calls[3][0]).toBe('http://127.0.0.1:9515/session/owned/element/button/click')
  await driver.close()
  expect(request.mock.lastCall?.[1].method).toBe('DELETE')
  expect(driver.session).toBeNull()
})

it('attaches only to an owned loopback WebView2 endpoint without changing the application', async () => {
  const request = vi.fn()
    .mockResolvedValueOnce({ ok: true, json: async () => ({ value: { sessionId: 'attached' } }) })
    .mockResolvedValue({ ok: true, json: async () => ({ value: null }) })
  const driver = new WindowsUiWebDriver('http://127.0.0.1:9515/', request)
  await expect(driver.attach('example.org:9223')).rejects.toThrow('UiGateInvalid')
  expect(request).not.toHaveBeenCalled()
  await driver.attach('127.0.0.1:9223')
  expect(JSON.parse(request.mock.calls[0][1].body).capabilities.alwaysMatch)
    .toEqual({ browserName: 'webview2', 'ms:edgeChromium': true, 'ms:edgeOptions': { debuggerAddress: '127.0.0.1:9223' } })
  await driver.close()
})

it('recognizes CDP version metadata without requiring a browser-level WebSocket', () => {
  expect(webViewDebuggerIsReady({ Browser: 'Edg/156.0', 'Protocol-Version': '1.3' })).toBe(true)
  expect(webViewDebuggerIsReady(null)).toBe(false)
  expect(webViewDebuggerIsReady({ webSocketDebuggerUrl: 'ws://127.0.0.1/' })).toBe(false)
  expect(webViewDebuggerIsReady({ Browser: 'unrelated service' })).toBe(false)
})
