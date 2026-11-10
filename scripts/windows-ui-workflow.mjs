import { setTimeout as delay } from 'node:timers/promises'

export async function waitForUi(driver, script, args = [], timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const result = await driver.execute(script, args)
    if (result) return result
    await delay(100)
  }
  throw new Error('UiGateTimeout: actual application UI did not reach the required state')
}

/** Real Native import, actual React controls and W3C input. No IPC mocks or
 * alternate renderer. Import is test setup through the same production command. */
export async function runWindowsUiWorkflow(driver, fixture) {
  await waitForUi(driver, 'return !!window.__TAURI_INTERNALS__?.invoke && !!document.querySelector("header")')
  await driver.execute(`
    window.__STARROOM_UI_GATE_FAILURES__=[];
    window.__STARROOM_UI_GATE_TIMINGS__=[];
    window.__STARROOM_UI_GATE_LAST_STATUS__='';
    const collect=()=>{
      for(const e of document.querySelectorAll('.notice,[role="alert"],.canvas-footer')){
        const text=e.textContent || '';
        if(/失敗|錯誤|poisoned|History commit failed/i.test(text)
          && window.__STARROOM_UI_GATE_FAILURES__.length<32){
          window.__STARROOM_UI_GATE_FAILURES__.push(text.slice(0,500));
        }
      }
      const status=document.querySelector('.canvas-footer')?.textContent || '';
      if(status!==window.__STARROOM_UI_GATE_LAST_STATUS__){
        window.__STARROOM_UI_GATE_LAST_STATUS__=status;
        const timing=status.match(/編輯至繪製 ([0-9]+) ms/);
        if(timing && window.__STARROOM_UI_GATE_TIMINGS__.length<128){
          window.__STARROOM_UI_GATE_TIMINGS__.push({
            control:window.__STARROOM_UI_GATE_CONTROL__ || null,
            phase:/互動/.test(status)?'interactive':'final',
            editToDrawMs:Number(timing[1])
          });
        }
      }
    };
    new MutationObserver(collect).observe(document.body,{childList:true,subtree:true,characterData:true});
  `)
  await driver.clickText('圖庫', 'header button')
  await waitForUi(driver, 'return !!document.querySelector(".library-actions button:not(:disabled)")')
  const libraryLayout = await driver.execute(`
    const main=document.querySelector('.library-browser'), button=main.querySelector('.library-actions button');
    const r=main.getBoundingClientRect(), b=button.getBoundingClientRect();
    return {width:r.width,height:r.height,column:getComputedStyle(main).gridColumnStart,
      searchReceivesPointer:document.elementFromPoint(b.x+b.width/2,b.y+b.height/2)===button};
  `)
  if (libraryLayout.width < 320 || libraryLayout.column !== '2' || !libraryLayout.searchReceivesPointer) {
    throw new Error('UiGateLayoutFailed: production Library is not physically usable: ' + JSON.stringify(libraryLayout))
  }
  const imported = await driver.execute(`
    const done = arguments[arguments.length - 1];
    window.__TAURI_INTERNALS__.invoke('library_import_folder', {root:null, paths:[arguments[0]]})
      .then(value => done({value}), error => done({nativeError:String(error)}));
  `, [fixture], true)
  if (imported.nativeError || !imported.value?.imported?.length || imported.value.failed?.length) {
    throw new Error('UiGateImportFailed: production Native import did not import the public fixture')
  }
  await driver.click('.library-actions button')
  await waitForUi(driver, 'return document.querySelectorAll(".photo-card-preview").length === 1')
  await driver.click('.photo-card-preview')
  await driver.clickText('編輯', 'header button')
  await driver.clickText('光線', '.tool-rail button')
  await waitForUi(driver, `
    const c=document.querySelector(".photo-stage canvas");
    const s=document.querySelector(".canvas-footer")?.textContent || '';
    return c && c.width>32 && c.height>32 && /原生/.test(s) && /最終品質/.test(s);
  `)
  const before = await driver.execute(canvasSignature)
  let previous = before
  let beforeLast = before
  const controls = []
  for (const label of ['曝光', '對比', '高光', '陰影', '白色', '黑色']) {
    beforeLast = previous
    await driver.execute('window.__STARROOM_UI_GATE_CONTROL__=arguments[0]', [label])
    const selector = `input[type="range"][aria-label="${label}"]`
    const element = await driver.element(selector)
    // One physical adjustment gesture per control. Four separate ArrowRight
    // presses are four undoable gestures, not one drag/history command.
    await driver.command(`/session/${driver.session}/element/${element}/value`, {
      text: '\uE00E', value: ['\uE00E'],
    })
    const current = await waitForUi(driver, `
      const s=document.querySelector(".canvas-footer")?.textContent || '';
      const input=document.querySelector(arguments[0]);
      if(!Number(input?.value) || !/最終品質/.test(s) || !/編輯至繪製/.test(s) || /失敗|錯誤/.test(s)) return null;
      const frame=(()=>{${canvasSignature}})();
      return frame.signature!==arguments[1] ? frame : null;
    `, [selector, previous.signature])
    const value = await driver.execute('return Number(document.querySelector(arguments[0]).value)', [selector])
    controls.push({ label, value, pixelsChanged: true })
    previous = current
  }
  const after = await driver.execute(canvasSignature)
  if (before.signature === after.signature) {
    throw new Error('UiGateRenderFailed: actual control edits did not change displayed pixels')
  }
  if (before.width !== after.width || before.height !== after.height) {
    throw new Error('UiGateRenderFailed: final preview dimensions drifted after tone edits')
  }
  await driver.click('.photo-stage canvas')
  const shortcut = async (redo) => {
    await driver.actions([{ type: 'key', id: 'keyboard', actions: [
      { type: 'keyDown', value: '\uE009' },
      ...(redo ? [{ type: 'keyDown', value: '\uE008' }] : []),
      { type: 'keyDown', value: 'z' }, { type: 'keyUp', value: 'z' },
      ...(redo ? [{ type: 'keyUp', value: '\uE008' }] : []),
      { type: 'keyUp', value: '\uE009' },
    ] }])
  }
  const waitSignature = async (signature) => waitForUi(driver, `
    if(!/最終品質/.test(document.querySelector('.canvas-footer')?.textContent || '')) return false;
    return (()=>{${canvasSignature}})().signature===arguments[0];
  `, [signature])
  await shortcut(false)
  await waitSignature(beforeLast.signature)
  await shortcut(true)
  await waitSignature(after.signature)
  const errors = await driver.execute(`
    return window.__STARROOM_UI_GATE_FAILURES__;
  `)
  const timings = await driver.execute('return window.__STARROOM_UI_GATE_TIMINGS__')
  if (errors.length) throw new Error('UiGateApplicationFailed: application reported a typed failure')
  return { scope: 'Production WebView2 UI + Native import and tone controls, not full installed/latency acceptance',
    libraryLayout, controls, dimensions: [after.width, after.height], pixelsChanged: true,
    undoRedoDeterministic: true, unexpectedAlerts: errors.length,
    functionalProbeTimings: timings,
    timingScope: 'UI handler-to-Canvas draw under functional-test readbacks; not physical input/presentation or a standalone latency gate' }
}

const canvasSignature = `
  const canvas=document.querySelector('.photo-stage canvas');
  if(!canvas) throw new Error('Actual preview canvas missing');
  const data=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data;
  let hash=2166136261;
  for(let i=0;i<data.length;i++){hash=Math.imul(hash^data[i],16777619)>>>0}
  return {width:canvas.width,height:canvas.height,signature:hash};
`
