// VAPP-91: binding sources. A data model path can follow a host source
// (`exp:issues?board=…`): the host registers one resolver per scheme, the
// router emits a `bind` op, the resolver's emits land at the path.

import { sortKeys } from "./policy"

export interface ParsedSource {
  /** The source as written. */
  uri: string
  scheme: string
  name: string
  params: Record<string, string>
}

const SOURCE = /^([a-zA-Z][a-zA-Z0-9+.-]*):([^?]+)(?:\?(.*))?$/

function decode(part: string): string {
  try {
    return decodeURIComponent(part.replace(/\+/g, ` `))
  } catch {
    return part
  }
}

/** `exp:issues?board=b1&limit=20` → `{scheme: "exp", name: "issues",
 *  params: {board: "b1", limit: "20"}}`; null when it does not parse. */
export function parseSource(uri: string): ParsedSource | null {
  const m = SOURCE.exec(uri.trim())
  if (!m) return null
  const name = decode(m[2]!)
  if (!name) return null
  const params: Record<string, string> = {}
  for (const pair of (m[3] ?? ``).split(`&`)) {
    if (!pair) continue
    const eq = pair.indexOf(`=`)
    const key = decode(eq < 0 ? pair : pair.slice(0, eq))
    if (!key) continue
    params[key] = eq < 0 ? `` : decode(pair.slice(eq + 1))
  }
  return { uri, scheme: m[1]!.toLowerCase(), name, params: sortKeys(params) }
}

/** What a resolver emits into: the value lands at the bound path. */
export type SourceEmit = (value: unknown) => void

/** A scheme's resolver: start following `source`, return the cancel. */
export type SourceResolver = (source: ParsedSource, emit: SourceEmit) => (() => void) | void

export type SourceResolvers = Record<string, SourceResolver>
