// Pure core of the runConfigs router + editor UI: the server-side cwd/env
// validation rules and the argv <-> command-line helpers the web editor uses.
// Run configs are DB-stored argv the desktop apps spawn locally, which
// reverses the never-execute-synced-values invariant — the compensating
// control lives client-side (the per-device Trust & Run commandSetHash
// prompt), so the server's job here is defence in depth: keep cwd inside the
// checkout and strip loader-hijack environment keys.

export const MAX_RUN_CONFIG_NAME = 255
export const MAX_ARGV_ITEMS = 64
export const MAX_ARG_LENGTH = 1024
export const MAX_CWD_LENGTH = 512
export const MAX_ENV_ENTRIES = 64
export const MAX_ENV_KEY_LENGTH = 128
export const MAX_ENV_VALUE_LENGTH = 2048

// cwd must stay inside the checkout: relative, no `..` segments — the same
// rule the legacy PlatformCommon `rootDir`/`cwd` documented. Callers pass a
// trimmed, non-empty string (empty/null means repo root).
export function runConfigCwdError(cwd: string): string | null {
  if (cwd.startsWith(`/`) || cwd.startsWith(`\\`) || /^[A-Za-z]:[\\/]/.test(cwd)) {
    return `cwd must be a relative path inside the repository`
  }
  if (cwd.split(/[\\/]+/).some((segment) => segment === `..`)) {
    return `cwd must not contain ".." segments`
  }
  return null
}

// Keys that would let a stored config hijack the spawned process beyond its
// own argv: PATH swaps the resolved binary, LD_PRELOAD/DYLD_* inject code
// into it. Case-insensitive to be safe across platforms.
export function isBlockedEnvKey(key: string): boolean {
  const upper = key.toUpperCase()
  return (
    upper === `PATH` || upper === `LD_PRELOAD` || upper.startsWith(`DYLD_`)
  )
}

// Strip (not reject) blocked keys — mirrors the documented PlatformCommon
// convention ("PATH/LD_PRELOAD/DYLD_* are stripped server-side").
export function sanitizeRunConfigEnv(
  env: Record<string, string>
): Record<string, string> {
  const out: Record<string, string> = {}
  for (const [key, value] of Object.entries(env)) {
    if (isBlockedEnvKey(key)) continue
    out[key] = value
  }
  return out
}

// ---------------------------------------------------------------------------
// argv <-> single editable command line
// ---------------------------------------------------------------------------
//
// The editor shows argv as one monospace line. Tokenization is shell-LIKE
// (whitespace-separated; '…' literal; "…" with \" and \\ escapes; backslash
// escapes the next char outside quotes) but nothing is ever run through a
// shell — the desktops spawn argv as-is.

export function parseArgvLine(line: string): string[] {
  const argv: string[] = []
  let current = ``
  let inToken = false
  let quote: `'` | `"` | null = null

  for (let i = 0; i < line.length; i++) {
    const ch = line[i]!
    if (quote === `'`) {
      if (ch === `'`) quote = null
      else current += ch
    } else if (quote === `"`) {
      if (ch === `\\` && (line[i + 1] === `"` || line[i + 1] === `\\`)) {
        current += line[i + 1]!
        i++
      } else if (ch === `"`) {
        quote = null
      } else {
        current += ch
      }
    } else if (ch === `'` || ch === `"`) {
      quote = ch
      inToken = true
    } else if (ch === `\\` && i + 1 < line.length) {
      current += line[i + 1]!
      inToken = true
      i++
    } else if (/\s/.test(ch)) {
      if (inToken) {
        argv.push(current)
        current = ``
        inToken = false
      }
    } else {
      current += ch
      inToken = true
    }
  }
  // An unterminated quote just consumes the rest of the line — the editor is
  // forgiving; the round-trip form formatArgvLine emits is always terminated.
  if (inToken) argv.push(current)
  return argv
}

// Round-trips through parseArgvLine: plain args stay bare, anything with
// whitespace/quotes/backslashes is double-quoted with \" and \\ escapes.
export function formatArgvLine(argv: string[]): string {
  return argv
    .map((arg) => {
      if (arg.length > 0 && !/[\s'"\\]/.test(arg)) return arg
      return `"${arg.replace(/[\\"]/g, (c) => `\\${c}`)}"`
    })
    .join(` `)
}
