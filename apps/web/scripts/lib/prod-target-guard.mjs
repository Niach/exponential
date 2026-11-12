// #EXP-1798: tool tests never touch prod data. On 2026-11-12 a session's MCP
// tool-test script deleted 16 real issues on app.exponential.at. Every
// repo harness that talks to an instance by URL (`--base`, `--bearer`) runs
// its base through here: no base = the local stack, a prod host = refused
// unless the caller typed `--i-know-this-is-prod`. The hosts and the flag
// are @exp/emcp `fixtures/deletes.json`'s (`prodHosts`, `prodFlag`).
//
// Plain .mjs so node scripts (Playwright's request client fails under Bun)
// and the TS scripts share it.
import fixture from "@exp/emcp/fixtures/deletes.json" with { type: "json" }

export const PROD_FLAG = fixture.prodFlag
export const DEFAULT_BASE = fixture.defaultBase

/** `ok` | `refused` | `invalid` (deletes.json `prodHostCases`). */
export function prodBaseVerdict(base, flag) {
  let host
  try {
    host = new URL(base).hostname.toLowerCase()
  } catch {
    return `invalid`
  }
  return fixture.prodHosts.includes(host) && !flag ? `refused` : `ok`
}

/**
 * The base a harness may run against: `base` (default: the local stack),
 * with any trailing slash dropped. Throws on a prod host without the flag in
 * `argv`, and on a base that is not a URL.
 */
export function assertToolTestBase(base, argv = process.argv) {
  const target = (base || DEFAULT_BASE).replace(/\/$/, ``)
  const verdict = prodBaseVerdict(target, argv.includes(PROD_FLAG))
  if (verdict === `invalid`) throw new Error(`--base "${target}" is not a URL`)
  if (verdict === `refused`) {
    throw new Error(
      `Refusing to run a tool test against ${new URL(target).host}: that is production, with real teams' data. ` +
        `Use staging (https://next.exponential.at) or a local stack (${DEFAULT_BASE}), or pass ${PROD_FLAG} if you truly mean prod.`
    )
  }
  return target
}
