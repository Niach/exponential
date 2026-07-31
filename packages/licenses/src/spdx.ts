// EXP-375 — SPDX expression parsing and the LICENCE ELECTION POLICY.
//
// This is the load-bearing part of the pipeline. A naive generator that just
// concatenates every id it sees would assert GPL terms over our own binary:
// `self_cell` declares `Apache-2.0 OR GPL-2.0-only`, and an `OR` is the
// licensor offering a CHOICE. We take exactly one branch and record which.
//
// `AND` is the opposite — every conjunct binds. `unicode-ident` is
// `(MIT OR Apache-2.0) AND Unicode-3.0`: the Unicode terms are mandatory no
// matter which of MIT/Apache we elect.
//
// Grammar (SPDX 2.3, `AND` binds tighter than `OR`):
//
//   or   := and ( 'OR' and )*
//   and  := unary ( 'AND' unary )*
//   unary:= '(' or ')' | id ( 'WITH' exception )?

import { byCodepoint } from "./schema"

export type Expr =
  | { kind: `id`; id: string; exception?: string }
  | { kind: `or`; branches: Expr[] }
  | { kind: `and`; parts: Expr[] }

/**
 * Election preference, best first. The ordering is a policy decision, not a
 * legal one — every id below is on `apps/desktop/deny.toml`'s allow-list, so
 * any of them is fine to accept; we simply prefer the shortest, most permissive
 * and most widely shared bodies so the aggregate dedupes well.
 *
 * What matters legally is what is ABSENT: `GPL-2.0-only`, `Unlicense`, `NCSA`
 * and friends have no rank, so a branch offering them is only ever elected when
 * there is nothing else on offer — and the cross-gate in
 * `apps/web/src/lib/licenses.test.ts` then fails, which is the point.
 */
export const ELECTION_ORDER = [
  `MIT`,
  `ISC`,
  `BSD-2-Clause`,
  `BSD-3-Clause`,
  `0BSD`,
  `MIT-0`,
  `Zlib`,
  `Apache-2.0`,
  `Apache-2.0 WITH LLVM-exception`,
  `BSL-1.0`,
  `CC0-1.0`,
  `MPL-2.0`,
  `Unicode-3.0`,
  `Unicode-DFS-2016`,
  `CDLA-Permissive-2.0`,
  `bzip2-1.0.6`,
]

/**
 * Legacy spellings that predate SPDX expressions and still sit in published
 * metadata. crates.io alone carries `MIT/Apache-2.0`, `MIT or Apache-2.0` and
 * `Apache-2.0 / MIT`; npm adds a few more. A `/` has always meant "or".
 */
const normaliseExpression = (raw: string): string =>
  raw
    .trim()
    .replace(/\s*\/\s*/g, ` OR `)
    .replace(/\bor\b/g, `OR`)
    .replace(/\band\b/g, `AND`)
    .replace(/\bwith\b/g, `WITH`)
    .replace(/\s+/g, ` `)

const tokenise = (expr: string): string[] =>
  expr
    .replace(/([()])/g, ` $1 `)
    .split(/\s+/)
    .filter(Boolean)

export function parseExpression(raw: string): Expr {
  const tokens = tokenise(normaliseExpression(raw))
  let pos = 0

  const peek = (): string | undefined => tokens[pos]
  const take = (): string => {
    const t = tokens[pos++]
    if (t === undefined) throw new Error(`unexpected end of SPDX expression`)
    return t
  }

  function parseOr(): Expr {
    const branches = [parseAnd()]
    while (peek() === `OR`) {
      take()
      branches.push(parseAnd())
    }
    return branches.length === 1 ? branches[0] : { kind: `or`, branches }
  }

  function parseAnd(): Expr {
    const parts = [parseUnary()]
    while (peek() === `AND`) {
      take()
      parts.push(parseUnary())
    }
    return parts.length === 1 ? parts[0] : { kind: `and`, parts }
  }

  function parseUnary(): Expr {
    if (peek() === `(`) {
      take()
      const inner = parseOr()
      if (take() !== `)`) throw new Error(`unbalanced ( in "${raw}"`)
      return inner
    }
    const id = take()
    if (id === `OR` || id === `AND` || id === `)`) {
      throw new Error(`malformed SPDX expression "${raw}"`)
    }
    if (peek() === `WITH`) {
      take()
      return { kind: `id`, id, exception: take() }
    }
    return { kind: `id`, id }
  }

  const parsed = parseOr()
  if (pos !== tokens.length) {
    throw new Error(`trailing tokens in SPDX expression "${raw}"`)
  }
  return parsed
}

/** `Apache-2.0 WITH LLVM-exception` renders as one id; everything else is bare. */
export const idOf = (e: Extract<Expr, { kind: `id` }>): string =>
  e.exception ? `${e.id} WITH ${e.exception}` : e.id

const rank = (id: string): number => {
  const i = ELECTION_ORDER.indexOf(id)
  // Unranked ids sort after every ranked one, then deterministically by name.
  return i >= 0 ? i : ELECTION_ORDER.length
}

/** Worst id in a branch decides how good the branch is. */
const branchRank = (ids: string[]): number => Math.max(...ids.map(rank))

export interface Resolution {
  /** Ids whose terms actually bind, sorted by codepoint. */
  licenses: string[]
  /** Set iff an `OR` was resolved: a sentence naming the elected branch. */
  election?: string
}

/**
 * Apply the election policy to a parsed expression.
 *
 * `A OR B` -> exactly one branch, recorded. `A AND B` -> both. Nested freely.
 */
export function resolve(expr: Expr, raw: string): Resolution {
  // `elections` is threaded through the return value rather than accumulated in
  // a closure: scoring an `OR` has to evaluate branches we will NOT take, and a
  // shared accumulator would record their nested elections too.
  interface Eval {
    ids: string[]
    elections: string[]
  }

  const walk = (e: Expr): Eval => {
    if (e.kind === `id`) return { ids: [idOf(e)], elections: [] }
    if (e.kind === `and`) {
      const parts = e.parts.map(walk)
      return {
        ids: parts.flatMap((p) => p.ids),
        elections: parts.flatMap((p) => p.elections),
      }
    }
    const scored = e.branches.map((b) => {
      const ev = walk(b)
      const ids = [...new Set(ev.ids)].sort(byCodepoint)
      return { ev, ids, key: ids.join(` AND `), r: branchRank(ids) }
    })
    scored.sort((a, b) => a.r - b.r || byCodepoint(a.key, b.key))
    const winner = scored[0]
    return {
      ids: winner.ids,
      elections: [winner.key, ...winner.ev.elections],
    }
  }

  const { ids, elections } = walk(expr)
  const licenses = [...new Set(ids)].sort(byCodepoint)
  if (elections.length === 0) return { licenses }
  return {
    licenses,
    election: `${elections[0]} elected from \`${raw.trim()}\``,
  }
}

/** Parse + resolve in one step. Throws on a malformed expression. */
export const electFrom = (raw: string): Resolution =>
  resolve(parseExpression(raw), raw)

/** Every id an expression mentions, elected or not — used by coverage gates. */
export function allIds(expr: Expr): string[] {
  if (expr.kind === `id`) return [idOf(expr)]
  const kids = expr.kind === `or` ? expr.branches : expr.parts
  return kids.flatMap(allIds)
}
