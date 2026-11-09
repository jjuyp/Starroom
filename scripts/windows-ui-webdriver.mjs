// W3C transport only. Microsoft Edge WebDriver can launch a production WebView2
// executable directly; no test plugin, renderer mock or localhost dev server.
export class WindowsUiWebDriver {
  constructor(baseUrl, request = fetch) {
    const parsed = new URL(baseUrl)
    if (parsed.protocol !== 'http:' || parsed.hostname !== '127.0.0.1'
      || parsed.username || parsed.password || parsed.pathname !== '/') {
      throw new Error('UiGateInvalid: WebDriver must be a loopback-only endpoint')
    }
    this.baseUrl = parsed.origin
    this.request = request
    this.session = null
  }

  async command(path, body, method = 'POST') {
    const response = await this.request(this.baseUrl + path, {
      method, headers: { 'content-type': 'application/json' },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      signal: AbortSignal.timeout(120_000),
    })
    const result = await response.json()
    if (!response.ok || result.value?.error) {
      throw new Error(`UiGateWebDriverFailed: ${result.value?.error ?? response.status}: ${result.value?.message ?? ''}`)
    }
    return result.value
  }

  async launch(executable) {
    if (this.session) throw new Error('UiGateInvalid: an application session is already active')
    const result = await this.command('/session', { capabilities: { alwaysMatch: {
      browserName: 'webview2', 'ms:edgeOptions': { binary: executable },
    } } })
    if (!result?.sessionId) throw new Error('UiGateWebDriverFailed: missing session id')
    this.session = result.sessionId
    await this.command(`/session/${this.session}/timeouts`, { script: 120_000, pageLoad: 120_000 })
  }

  async attach(debuggerAddress) {
    if (this.session || !/^127\.0\.0\.1:[0-9]+$/.test(debuggerAddress)) {
      throw new Error('UiGateInvalid: an unused loopback debugging endpoint is required')
    }
    const result = await this.command('/session', { capabilities: { alwaysMatch: {
      browserName: 'webview2', 'ms:edgeOptions': { debuggerAddress },
    } } })
    if (!result?.sessionId) throw new Error('UiGateWebDriverFailed: missing attached session id')
    this.session = result.sessionId
    await this.command(`/session/${this.session}/timeouts`, { script: 120_000, pageLoad: 120_000 })
  }

  async execute(script, args = [], async = false) {
    if (!this.session) throw new Error('UiGateInvalid: no application session')
    return this.command(`/session/${this.session}/execute/${async ? 'async' : 'sync'}`, { script, args })
  }

  async element(selector) {
    const result = await this.command(`/session/${this.session}/element`, {
      using: 'css selector', value: selector,
    })
    const element = result?.['element-6066-11e4-a52e-4f735466cecf']
    if (!element) throw new Error('UiGateWebDriverFailed: missing W3C element')
    return element
  }

  async click(selector) {
    const element = await this.element(selector)
    await this.command(`/session/${this.session}/element/${element}/click`, {})
  }

  async clickText(text, scope = 'button') {
    const result = await this.execute(
      'return Array.from(document.querySelectorAll(arguments[1])).find(e => e.textContent.trim() === arguments[0]) || null',
      [text, scope])
    const element = result?.['element-6066-11e4-a52e-4f735466cecf']
    if (!element) throw new Error('UiGateWebDriverFailed: requested visible button is missing')
    await this.command(`/session/${this.session}/element/${element}/click`, {})
  }

  async actions(actions) {
    await this.command(`/session/${this.session}/actions`, { actions })
  }

  async close() {
    if (!this.session) return
    const session = this.session
    this.session = null
    await this.command(`/session/${session}`, undefined, 'DELETE')
  }
}
