/**
 * #EXP-1868: list what a capture run creates OUTSIDE its session's cgroup (a
 * temp data dir, a simulator, an AVD, a compose project) in the session's
 * manifest, so the session's end deletes it even when this run is killed
 * before its own `finally` (the daemon's `coding::session_manifest`). The
 * same file and format as `scripts/lanes/session-manifest.sh`:
 * `<state>/exponential/session-manifests/<EXP_MCP_SESSION_ID>`, one
 * `<kind>\t<value>` line each. Outside a session nothing is recorded.
 */
import { appendFileSync, mkdirSync, readFileSync } from "node:fs"
import { homedir } from "node:os"
import { isAbsolute, join } from "node:path"

export type ManifestKind = `sim` | `avd` | `dir` | `compose`

export function sessionManifestPath(env: Record<string, string | undefined> = process.env): string | null {
  const session = env.EXP_MCP_SESSION_ID ?? ``
  if (!/^[A-Za-z0-9-]{1,64}$/.test(session)) return null
  const dir =
    env.EXP_SESSION_MANIFEST_DIR ||
    join(env.XDG_STATE_HOME || join(env.HOME || homedir(), `.local/state`), `exponential/session-manifests`)
  return join(dir, session)
}

/** Appends `kind value` once; false = not recorded (no session, bad value). */
export function sessionManifestAdd(
  kind: ManifestKind,
  value: string,
  env: Record<string, string | undefined> = process.env
): boolean {
  if (!value || /[\t\n]/.test(value) || (kind === `dir` && !isAbsolute(value))) return false
  const path = sessionManifestPath(env)
  if (!path) return false
  const line = `${kind}\t${value}`
  try {
    let existing = ``
    try {
      existing = readFileSync(path, `utf8`)
    } catch {}
    if (existing.split(`\n`).includes(line)) return true
    mkdirSync(join(path, `..`), { recursive: true })
    appendFileSync(path, `${line}\n`)
    return true
  } catch {
    return false
  }
}
