/** Native history follows changed render state, not the lifetime of a pointer/focus gesture.
 * A numeric editor may stay focused longer than the debounce, and a slider drag may pause.
 * Restored state is already acknowledged and must never create a new history command.
 */
export function nativeHistoryStateChanged<T>(acknowledged: T | undefined, current: T, restoring: boolean): boolean {
  return !restoring && acknowledged !== undefined && JSON.stringify(acknowledged) !== JSON.stringify(current)
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
