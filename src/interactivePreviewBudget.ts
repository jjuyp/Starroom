/** Presentation scheduling only; Native retains all image processing.
 * Start conservatively, downshift on sustained misses and upgrade only with
 * sustained headroom. Final/HQ requests never use this controller. */
export class InteractivePreviewBudget {
  private tier = 1024
  private fast = 0
  private slow = 0
  private gpu = true
  private sampledEdge = 0

  edge(displayEdge: number, sourceEdge: number): number {
    const demand = Number.isFinite(displayEdge) && displayEdge > 0 ? displayEdge : 1024
    const ceiling = demand <= 512 ? 512 : demand <= 1024 ? 1024 : 2048
    const source = Number.isFinite(sourceEdge) && sourceEdge > 0 ? sourceEdge : 2048
    return Math.max(256, Math.min(this.tier, ceiling, this.gpu ? 2048 : 1024, source))
  }

  observe(requestedEdge: number, elapsedMs: number, gpu: boolean): void {
    if (!Number.isFinite(elapsedMs) || elapsedMs <= 0) return
    this.gpu = gpu
    if (!gpu) this.tier = Math.min(this.tier, 1024)
    // A cost measured on a smaller source/viewport is a valid reason to go
    // smaller, never evidence that a larger tier will be affordable. Don't mix
    // streaks across sizes or learn from an in-flight superseded larger tier.
    if (requestedEdge > this.tier || !Number.isFinite(requestedEdge) || requestedEdge < 256) return
    if (this.sampledEdge !== requestedEdge) {
      this.sampledEdge = requestedEdge
      this.slow = this.fast = 0
    }
    // Target the 50ms interactive budget, leaving headroom before the 100ms
    // end-to-end ceiling. Waiting for an 80ms service time needlessly held the
    // measured D750 exposure case at 1024 despite a much faster 512 tier.
    this.slow = elapsedMs >= 50 ? this.slow + 1 : 0
    this.fast = elapsedMs < 12 && requestedEdge === this.tier ? this.fast + 1 : 0
    if (this.slow >= 2 && requestedEdge > 512) {
      this.tier = requestedEdge > 1024 ? 1024 : 512
      this.slow = this.fast = 0
    } else if (this.fast >= 8 && this.tier < (gpu ? 2048 : 1024)) {
      this.tier *= 2
      this.slow = this.fast = 0
    }
  }
}
