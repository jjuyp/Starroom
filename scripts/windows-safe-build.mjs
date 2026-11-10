import { execFileSync, spawnSync } from 'node:child_process'
import process from 'node:process'
import { withWindowsPathAlias } from './windows-path-alias.mjs'

const cwd = process.cwd()
const needsAsciiAlias = process.platform === 'win32' && /[^\x00-\x7F]/.test(cwd)

function runBuild(buildCwd) {
  const command = process.platform === 'win32' ? 'cmd.exe' : 'npm'
  const args = process.platform === 'win32'
    ? ['/d', '/s', '/c', 'npm.cmd run build:web']
    : ['run', 'build:web']
  const result = spawnSync(command, args, {
    cwd: buildCwd,
    stdio: 'inherit',
    shell: false,
  })
  if (result.error) throw result.error
  return result.status ?? 1
}

if (!needsAsciiAlias) {
  process.exit(runBuild(cwd))
}

process.exitCode = withWindowsPathAlias(cwd,
  (drive, path) => execFileSync('subst.exe', [drive, path], { stdio: 'pipe' }),
  (drive) => execFileSync('subst.exe', [drive, '/D'], { stdio: 'inherit' }),
  (path) => {
    console.log(`Starroom build: using owned ASCII alias ${path} for toolchain compatibility.`)
    return runBuild(path)
  })
