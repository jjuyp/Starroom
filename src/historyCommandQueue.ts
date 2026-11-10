/** Only changed render state is eligible for persistence. Gesture boundaries decide when it
 * commits, independently of preview quality or numeric-editor focus. Restored state is already
 * acknowledged and must never create a new history command.
 */
export function nativeHistoryStateChanged<T>(acknowledged: T | undefined, current: T, restoring: boolean): boolean {
  // serde_json::Value returns sorted object keys; JavaScript insertion order is not edit intent.
  // Preserve array order and JSON's omission of undefined optional properties.
  const canonicalJson = (value: T) => JSON.stringify(value, (key, child: unknown) => {
    if (child !== null && typeof child === 'object' && !Array.isArray(child)) {
      const projected: Record<string, unknown> = { ...child }
      // Older native History stores nested lens metadata using snake_case. Aliases are
      // equivalent edit intent, not an edit made merely by opening/restoring a photo.
      // Compare a projection only; the original acknowledged JSON/hash is left untouched.
      if (key === 'manualIdentity') for (const [legacy, canonical] of [
        ['camera_make', 'cameraMake'], ['camera_model', 'cameraModel'],
        ['lens_make', 'lensMake'], ['lens_model', 'lensModel'],
        ['focal_length_mm', 'focalLengthMm'], ['focus_distance_m', 'focusDistanceM'],
      ]) {
        if (legacy in projected && !(canonical in projected)) {
          projected[canonical] = projected[legacy]
          delete projected[legacy]
        }
      }
      return Object.fromEntries(Object.entries(projected).sort(([left], [right]) => left.localeCompare(right)))
    }
    return child
  })
  return !restoring && acknowledged !== undefined && canonicalJson(acknowledged) !== canonicalJson(current)
}

export const adjustmentKeys = new Set([
  'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown',
])

/** Tracks physical input lifetime, not an idle timeout. Begin is called only by an editor;
 * unrelated clicks therefore cannot suppress autosave. Focus alone never creates a hold.
 */
export class HistoryGestureBoundary {
  private down = new Set<string>()
  private held = new Set<string>()
  private latestInput: string | null = null

  inputDown(input: string) { this.down.add(input); this.latestInput = input }
  inputUp(input: string) { this.down.delete(input); this.held.delete(input) }
  begin() { if (this.latestInput !== null && this.down.has(this.latestInput)) this.held.add(this.latestInput) }
  get holding() { return this.held.size > 0 }
  get commitDelay(): number | null { return this.holding ? null : 220 }
  clear() { this.down.clear(); this.held.clear(); this.latestInput = null }
}

/** Serialize native history commands so each commit uses the acknowledged server state. */
export class HistoryCommandQueue {
  private tail: Promise<unknown> = Promise.resolve()
  run<T>(command: () => Promise<T>): Promise<T> {
    const result = this.tail.then(command)
    this.tail = result.catch(() => undefined)
    return result
  }
  async idle() { await this.tail }
}
