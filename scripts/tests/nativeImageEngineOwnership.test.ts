import { describe, expect, it } from 'vitest'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, readdirSync } from 'node:fs'

describe('Native engine source ownership and immutable migration corpus', () => {
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
