/**
 * The dev-server Node version guard, as two pure functions (EXP-632).
 *
 * On Node 26, `fetch` over a UNIX SOCKET resolves with the right status and the
 * right body and ZERO response headers (the same server over TCP is fine — an
 * undici regression). Nitro's dev worker is reached over exactly such a socket,
 * so every TanStack Start route is served with all of its headers stripped:
 * no `content-type` (Start itself then throws `expected content-type header to
 * be set` in the browser), no `set-cookie` (login cannot work), and no
 * `electric-handle`/`electric-offset` — which ARE the shape cursor, so every
 * Electric client re-requests `offset=-1` forever and no collection ever syncs.
 * Nothing about it looks like an error: the JSON is correct, the app renders,
 * and it just never fills in. It cost a full screenshot-store refresh (EXP-566)
 * before anyone noticed.
 *
 * `.tool-versions` already pins the supported major; this is the check that
 * makes the pin bite on machines with no version manager installed. It lives
 * here, free of `process` and the filesystem, so the message and the boundary
 * conditions are unit-testable — `vite.config.ts` supplies the real values.
 */

/** The `nodejs` major pinned in a `.tool-versions` file; null when absent. */
export function pinnedNodeMajor(toolVersions: string): number | null {
  const match = toolVersions.match(/^nodejs\s+(\d+)/m)
  return match ? Number(match[1]) : null
}

export interface NodeGuardVerdict {
  kind: `ok` | `warn` | `fail`
  /** Present on warn/fail, and on the Bun note. */
  message?: string
}

/** The first Node major whose undici strips unix-socket response headers. */
const BROKEN_NODE_MAJOR = 26

export function nodeGuardVerdict(opts: {
  /** `process.versions.node` — a full version string. */
  current: string
  /** From `pinnedNodeMajor`; null when `.tool-versions` is missing/odd. */
  pinned: number | null
  /** True under Bun (`process.versions.bun`) — Bun's fetch is unaffected. */
  isBun: boolean
}): NodeGuardVerdict {
  const { current, pinned, isBun } = opts
  if (isBun) {
    // Vite under Bun still reports a `process.versions.node` (the emulated
    // one), which has nothing to do with the runtime actually serving.
    return {
      kind: `ok`,
      message: `[vite] running under Bun — the Node unix-socket header bug does not apply.`,
    }
  }

  const major = Number(current.split(`.`)[0])
  const pin = pinned === null ? `the version in .tool-versions` : `Node ${pinned}`

  if (Number.isFinite(major) && major >= BROKEN_NODE_MAJOR) {
    return {
      kind: `fail`,
      message: [
        `Node ${current} cannot serve this app in dev.`,
        `  Node ${BROKEN_NODE_MAJOR} drops EVERY response header on unix-socket fetches, which is how`,
        `  nitro's dev worker is reached — so Electric never syncs, login cannot set a`,
        `  cookie, and the app renders but stays empty, with no error anywhere.`,
        `  Use ${pin} (\`.tool-versions\`), e.g. with Homebrew:`,
        `    PATH="/opt/homebrew/opt/node@${pinned ?? 24}/bin:$PATH" bun dev`,
        `  Or serve the production build, which runs under Bun and is unaffected:`,
        `    bun run build && PORT=5173 bun --env-file=.env .output/server/index.mjs`,
      ].join(`\n`),
    }
  }

  if (pinned !== null && major === pinned) return { kind: `ok` }

  return {
    kind: `warn`,
    message: `[vite] Node ${current} is not ${pin} (.tool-versions) — dev serving is unverified on it.`,
  }
}
