import { describe, expect, it } from 'vitest'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, readdirSync } from 'node:fs'

describe('Native engine source ownership and immutable migration corpus', () => {
  it('retains pinned Fast Float sources and bundles its distinct GPL notice', () => {
    const root = new URL('../../vendor/lcms-fast-float-2.19/', import.meta.url)
    const manifest = readFileSync(new URL('README.md', root), 'utf8')
    const entries = [...manifest.matchAll(/^- `([^`]+)`: `([a-f0-9]{64})`/gm)]
    expect(entries).toHaveLength(17)
    for (const [, path, hash] of entries) {
      const content = readFileSync(new URL(path, root), 'utf8').replaceAll('\r\n', '\n')
      expect(createHash('sha256').update(content).digest('hex'), path).toBe(hash)
    }
    const build = readFileSync(new URL('../../crates/starroom-color-management/build.rs', import.meta.url), 'utf8')
    for (const [, path] of entries.filter(([, path]) => path.endsWith('.c'))) {
      expect(build).toContain(`"${path.slice(4, -2)}"`)
    }
    expect(build).toContain('DEP_LCMS2_INCLUDE')
    const licenses = readFileSync(new URL('../../THIRD_PARTY_LICENSES.txt', import.meta.url), 'utf8')
    expect(licenses).toContain('LittleCMS Fast Float 2.19 / 21c582a594fe5279f90c0b93437c398f93bf62b0 / GPL-3.0-or-later')
    expect(licenses).toContain(readFileSync(new URL('COPYING.GPL3', root), 'utf8').replaceAll('\r\n', '\n').trimEnd())
  })
  it('dispatches actual file and CPU commands through the asynchronous native worker boundary', () => {
    const native = readFileSync(new URL('../../src-tauri/src/lib.rs', import.meta.url), 'utf8')
    const worker = readFileSync(new URL('../../src-tauri/src/native_worker.rs', import.meta.url), 'utf8')
    for (const [command, operation] of [
      ['ai_availability_status', 'AiAvailability'], ['native_optics_status', 'NativeOptics'],
      ['native_sample_color', 'NativeColorSample'], ['native_reference_match', 'NativeReference'],
    ]) {
      expect(native).toMatch(new RegExp(`#\\[tauri::command\\]\\s+async fn ${command}\\(`))
      const body = native.split(`async fn ${command}(`)[1].split(`fn ${command}_inner(`)[0]
      expect(body).toContain(`native_worker::run("${operation}"`)
      expect(body).toContain(`${command}_inner(`)
      expect(native).toContain(`            ${command},`)
    }
    expect(worker).toContain('tauri::async_runtime::spawn_blocking(work)')
    expect(worker).toContain('WorkerFailed: {error}')
  })
  it('uses payload histograms in the real preview without forcing a CPU-readback canvas', () => {
    const app = readFileSync(new URL('../../src/App.tsx', import.meta.url), 'utf8')
    expect(app).toContain('nativePreviewDisplayHistogram(nativeResult, readback)')
    expect(app).not.toContain('willReadFrequently: true')
  })
  it('has no unused direct JPEG writer bypassing professional export policy', () => {
    const native = readFileSync(new URL('../../src-tauri/src/lib.rs', import.meta.url), 'utf8')
    const bridge = readFileSync(new URL('../../src/nativeRender.ts', import.meta.url), 'utf8')
    const app = readFileSync(new URL('../../src/App.tsx', import.meta.url), 'utf8')
    expect(/\bfn\s+native_export_jpeg\s*\(/.test(native), 'retired direct JPEG command').toBe(false)
    expect(native.includes('std::fs::write(&request.output_path'), 'unsafe direct photo writer').toBe(false)
    expect(/\bfunction\s+(exportNativeJpeg|chooseNativeExportPath)\s*\(/.test(bridge), 'unused bridge/picker').toBe(false)
    expect(native).toContain('native_export_batch,')
    expect(native).toContain('export_one_with_destination_guard(')
    expect(bridge).toContain("invoke<NativeBatchExportResult>('native_export_batch'")
    expect(app).toContain('exportNativeLibraryBatch(')
    expect(app).toContain('exportNativeBatch(')
  })
  it('preserves exact frozen Browser observations and tolerances', () => {
    const text = readFileSync(new URL('../../tests/fixtures/m1c/browser-native-reference.json', import.meta.url), 'utf8').replace(/\r\n/g, '\n')
    expect(createHash('sha256').update(text).digest('hex')).toBe('f618d1e873d2433ae39f05297ebd425e347cdb3eab3c8d14ed1707bbb70fdb47')
  })
  it('does not retain a second creative engine and keeps every migrated Native assertion', () => {
    expect(existsSync(new URL('../../src/imagePipeline.ts', import.meta.url))).toBe(false)
    const creative = /\b(processImageData|renderImageToCanvas|mapToneCurve|srgbToLinear|linearToSrgb)\s*\(/
    for (const name of readdirSync(new URL('../../src/', import.meta.url), { recursive: true })) {
      if (!/\.tsx?$/.test(name) || /\.test\.tsx?$/.test(name)) continue
      expect(readFileSync(new URL(`../../src/${name.replaceAll('\\', '/')}`, import.meta.url), 'utf8'), name).not.toMatch(creative)
    }
    const tests = readFileSync(new URL('../../crates/starroom-pipeline/tests/legacy_browser_intents.rs', import.meta.url), 'utf8')
    for (const name of ['legacy_neutral_adjustments_remain_pixel_exact', 'legacy_exposure_changes_all_rendered_channels',
      'legacy_curve_controls_change_actual_native', 'legacy_monotone_curve_intent', 'legacy_shadow_lift_targets_dark_pixels',
      'legacy_black_anchor_survives_native_shadows', 'legacy_sharpness_produces_a_visible_native_edge_change',
      'legacy_relative_temperature_warms_encoded_native_gray']) expect(tests).toContain(`fn ${name}`)
  })
})
