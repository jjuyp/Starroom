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
