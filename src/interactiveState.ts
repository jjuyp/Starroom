export const MAX_INTERACTIVE_HISTORY = 20

export const appendInteractiveHistory = <T,>(history: T[], entry: T) =>
  [...history, entry].slice(-MAX_INTERACTIVE_HISTORY)

export const prependInteractiveHistory = <T,>(history: T[], entry: T) =>
  [entry, ...history].slice(0, MAX_INTERACTIVE_HISTORY)

export function scrollFilmstripFromWheel(
  filmstrip: Pick<HTMLElement, 'clientWidth' | 'scrollWidth' | 'scrollLeft'>,
  deltaX: number,
  deltaY: number,
) {
  if (filmstrip.scrollWidth <= filmstrip.clientWidth || deltaY === 0 || Math.abs(deltaX) >= Math.abs(deltaY)) return false
  const previous = filmstrip.scrollLeft
  filmstrip.scrollLeft += deltaY
  return filmstrip.scrollLeft !== previous
}
