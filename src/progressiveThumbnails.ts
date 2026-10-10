/** Bounded, progressive cache loading. A changed query stops scheduling obsolete work. */
export async function loadProgressiveThumbnails(
  ids: readonly number[], load: (id: number) => Promise<string>,
  publish: (id: number, url: string) => void, failed: (id: number, error: unknown) => void,
  active: () => boolean, concurrency = 3,
) {
  let index = 0
  await Promise.all(Array.from({ length: Math.min(ids.length, Math.max(1, concurrency)) }, async () => {
    while (active() && index < ids.length) {
      const id = ids[index++]
      try {
        const url = await load(id)
        if (active()) publish(id, url)
      } catch (error) { if (active()) failed(id, error) }
    }
  }))
}
