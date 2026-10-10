/** Claim with SUBST itself, never check-then-create. Cleanup only a successful owned claim. */
export function withWindowsPathAlias(cwd, map, unmap, run, letters = ['R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z']) {
  let drive
  for (const letter of letters) {
    const candidate = `${letter}:`
    try {
      map(candidate, cwd)
      drive = candidate
      break
    } catch {
      // A concurrent owner or real drive may occupy this letter. Never remove its mapping.
    }
  }
  if (!drive) throw new Error('Starroom build: no Windows ASCII path alias could be acquired.')
  try {
    return run(`${drive}\\`)
  } finally {
    unmap(drive)
  }
}
