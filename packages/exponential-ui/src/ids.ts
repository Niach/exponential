// VAPP-289 (round 9 contract §4, §14.3): identifiers. `isIdentifier` is
// UAX #31 over the GENERATED Unicode 17.0.0 tables (generated/xid.ts), the
// same tables every core reads; `mangle` turns any string into one (the id
// re-key of upgrade09 and the flat tool / upstream function names, §6.3).

import { isIdentifier, isXidContinue, isXidStart } from "../generated/xid"

export { isIdentifier, isXidContinue, isXidStart }

/** §14.3: every code point outside XID_Continue → `_`; empty → `_`; a first
 *  code point that is neither XID_Start nor `_` gets a `_` in front. */
export function mangle(s: string): string {
  let out = ``
  for (const ch of s) out += isXidContinue(ch.codePointAt(0)!) ? ch : `_`
  if (out === ``) return `_`
  const first = out.codePointAt(0)!
  return first === 0x5f || isXidStart(first) ? out : `_${out}`
}
