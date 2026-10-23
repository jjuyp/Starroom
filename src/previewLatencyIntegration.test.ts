import { afterEach, expect, it, vi } from 'vitest'
import { defaultAdjustments } from './editorState'
import { renderNativePreview } from './nativeRender'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true, convertFileSrc: (path: string) => path }))
afterEach(() => { vi.restoreAllMocks(); invoke.mockReset() })

it('measures each real preview queue dispatch rather than charging queued time to Native', async () => {
  let clock = 10
  vi.spyOn(performance, 'now').mockImplementation(() => clock)
  const completed: Array<(frame: Uint8Array) => void> = []
  invoke.mockImplementation(() => new Promise<Uint8Array>((resolve) => completed.push(resolve)))
  const surface = {}
  const render = () => renderNativePreview('private-local.nef', defaultAdjustments, [],
    { x: .5, y: .5, width: .42, height: .42, rotation: 0 },
    undefined, undefined, undefined, undefined, undefined, 1024, undefined, undefined, 'interactive', surface)
  const frame = new Uint8Array(44), view = new DataView(frame.buffer)
  frame.set([83, 82, 80, 52])
  view.setUint16(4, 4, true); view.setUint16(6, 8, true)
  for (const offset of [8, 12, 16, 20]) view.setUint32(offset, 1, true)
  view.setUint16(34, 1, true); view.setUint32(36, 4, true)
  frame.set([20, 30, 40, 255], 40)
  const first = render()
  clock = 20
  const second = render()
  expect(invoke).toHaveBeenCalledTimes(1)
  clock = 70
  completed[0](frame)
  expect((await first).timing).toEqual({ queuedAt: 10, dispatchedAt: 10, receivedAt: 70 })
  expect(invoke).toHaveBeenCalledTimes(2)
  clock = 90
  completed[1](frame)
  const result = await second
  expect(result.timing).toEqual({ queuedAt: 20, dispatchedAt: 70, receivedAt: 90 })
  expect(result.payload.bytes).toEqual(new Uint8Array([20, 30, 40, 255]))
  expect(invoke.mock.calls[0][1].request).not.toHaveProperty('timing')
})
