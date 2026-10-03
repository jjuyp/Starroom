/** Native history follows changed render state, not the lifetime of a pointer/focus gesture.
 * A numeric editor may stay focused longer than the debounce, and a slider drag may pause.
 * Restored state is already acknowledged and must never create a new history command.
 */
export function nativeHistoryStateChanged<T>(acknowledged: T | undefined, current: T, restoring: boolean): boolean {
  // serde_json::Value returns sorted object keys; JavaScript insertion order is not edit intent.
  // Preserve array order and JSON's omission of undefined optional properties.
  const canonicalJson = (value: T) => JSON.stringify(value, (_key, child: unknown) => {
    if (child !== null && typeof child === 'object' && !Array.isArray(child)) {
      return Object.fromEntries(Object.entries(child).sort(([left], [right]) => left.localeCompare(right)))
    }
    return child
  })
  return !restoring && acknowledged !== undefined && canonicalJson(acknowledged) !== canonicalJson(current)
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
