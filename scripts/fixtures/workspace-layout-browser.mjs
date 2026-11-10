// Browser-only layout diagnostic against the real production React/CSS page.
// This does not mock Native processing or qualify image quality/installed latency.
// Load directly with playwright-cli run-code --filename (no Node imports in its VM).
async (page) => {
  const read = () => page.evaluate(() => {
    const workspace = document.querySelector('.workspace')
    const main = document.querySelector('.library-browser')
    const button = main?.querySelector('.library-actions button')
    const panel = document.querySelector('.library-panel')
    if (!workspace || !main || !button || !panel) throw new Error('Actual Library layout missing')
    const rect = main.getBoundingClientRect(), target = button.getBoundingClientRect()
    const drawer = panel.getBoundingClientRect()
    return {
      mainWidth: rect.width, mainHeight: rect.height, mainColumn: getComputedStyle(main).gridColumnStart,
      searchReceivesPointer: document.elementFromPoint(target.x + target.width / 2,
        target.y + target.height / 2) === button,
      drawerVisible: drawer.right > 1 && drawer.left < innerWidth
        && getComputedStyle(panel).opacity === '1' && getComputedStyle(panel).pointerEvents !== 'none',
      collapsed: workspace.classList.contains('left-collapsed'),
      inspectorOpen: workspace.classList.contains('library-inspector-open'),
      inspectorColumn: getComputedStyle(document.querySelector('.inspector-panel')).gridColumnStart,
    }
  })
  const library = page.getByRole('button', { name: '圖庫', exact: true })
  const theme = page.getByRole('combobox', { name: '主題', exact: true })
  const setLeft = async (open) => {
    const collapsed = await page.locator('.workspace').evaluate((e) => e.classList.contains('left-collapsed'))
    if (open === collapsed) {
      await library.click()
      await page.keyboard.press('Backslash')
    }
  }
  await page.setViewportSize({ width: 1030, height: 750 })
  await theme.selectOption({ label: '深色' })
  await library.click()
  await setLeft(false)
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.library-panel')).opacity === '0')
  if ((await read()).inspectorOpen) await page.getByRole('button', { name: '收合照片資訊', exact: true }).click()
  await setLeft(true)
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.library-panel')).opacity === '1')
  // Reproduce the exact old auto-placement defect, then restore production CSS.
  const red = await page.evaluate(() => {
    const override = document.createElement('style')
    override.textContent = '.workspace > .library-panel,.workspace > .library-browser,.workspace > .canvas-area,.workspace > .inspector-panel{grid-column:auto;grid-row:auto}'
    document.head.append(override)
    try {
      const main = document.querySelector('.library-browser'), button = main.querySelector('.library-actions button')
      const target = button.getBoundingClientRect()
      return { contentWidth: main.clientWidth - 48, interceptedBy:
        document.elementFromPoint(target.x + target.width / 2, target.y + target.height / 2)?.className }
    } finally { override.remove() }
  })
  if (red.contentWidth !== 0 || red.interceptedBy !== 'library-panel') {
    throw new Error('LayoutProbeInvalid: original responsive Library defect was not reproduced')
  }
  const cases = []
  for (const label of ['深色', '灰色', '淺色']) {
    await theme.selectOption({ label })
    for (const width of [920, 1030, 1280, 1920]) {
      await page.setViewportSize({ width, height: 900 })
      for (const inspector of [false, true]) {
        await setLeft(false)
        const currentlyOpen = (await read()).inspectorOpen
        if (currentlyOpen !== inspector) await page.getByRole('button', { name: /^(照片資訊／關鍵字|收合照片資訊)$/ }).click()
        for (const drawer of [false, true]) {
          await setLeft(drawer)
          // Wait for real CSS transition completion, not a fixed arbitrary delay.
          await page.waitForFunction((visible) => {
            const e = document.querySelector('.library-panel'), r = e.getBoundingClientRect()
            return visible ? r.right > 1 && getComputedStyle(e).opacity === '1'
              : getComputedStyle(e).opacity === '0'
          }, drawer, { timeout: 2500 }).catch(async (error) => {
            throw new Error('LayoutProbeTransitionFailed: ' + JSON.stringify({ label, width,
              inspector, drawer, actual: await read(), reason: String(error) }))
          })
          const result = await read()
          if (result.mainWidth < 320 || result.mainHeight < 300 || result.mainColumn !== '2'
            || result.inspectorColumn !== '3' || !result.searchReceivesPointer
            || result.drawerVisible !== drawer || result.collapsed === drawer) {
            throw new Error('LayoutProbeFailed: ' + JSON.stringify({ label, width, inspector, drawer, ...result }))
          }
          cases.push({ theme: label, width, inspector, drawer, ...result })
        }
      }
    }
  }
  await setLeft(false)
  await theme.selectOption({ label: '深色' })
  await page.setViewportSize({ width: 1030, height: 750 })
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.library-panel')).opacity === '0')
  if ((await read()).inspectorOpen) await page.getByRole('button', { name: '收合照片資訊', exact: true }).click()
  const report = { scope: 'Production React/CSS layout only; not Native functionality or latency',
    generatedAt: new Date().toISOString(), originalDefect: red, cases }
  await page.screenshot({ path: 'output/playwright/workspace-layout.png' })
  return report
}
