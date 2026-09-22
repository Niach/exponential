import { contract } from "@exp/domain-contract"

// EXP-988 contract / EXP-920 implementation: what an Exponential MCP tool
// RESULT points at, so a run transcript can render a settled tool row as
// chips with hover cards instead of raw JSON.
//
// `toolResultPreview` is pure and is THE rule (fixture-locked ×2 by
// `packages/domain-contract/fixtures/tool-result-preview.json`): it reads the
// tool's parsed result (the JSON the `ok()` helper serialized) and returns the
// entities worth a chip, driven by the contract's per-tool spec
// (`expToolPreview.tools`, `kind@path` terms). It runs where the raw answer
// is: the engine (`crates/engine/src/mapper.rs` `exp_tool_refs`) distils the
// refs on the device and publishes them on the `tool_update` preview
// (`refs`), so every client — web, desktop, iOS, Android — renders the SAME
// chips from the same refs and resolves the hover card (an issue's status, a
// board's issues, a comment's body) from its own synced rows. `title` /
// `identifier` ride along as the display fallback for a row the viewer has
// not synced (a brand-new issue, another team's board).
//
// There is deliberately NO href on a ref: a result carries ids, never team or
// board slugs, and three of the four clients have no URLs at all — each
// client resolves the target from its rows (`useEntityRefTarget` on web).
//
// A list answer yields ONE `list` ref (its `id` = the member kind, `title` =
// the plural noun, `count` = the row count) followed by at most
// `maxRefs - 1` member refs, so a hover card can list what was listed.
// preview.test.ts lists EVERY registered tool with the kinds its result
// yields; a tool added later fails there until it is mapped.

export const entityRefKinds = contract.entityRefKind.values as readonly [
  `issue`,
  `board`,
  `action`,
  `automation`,
  `comment`,
  `session`,
  `label`,
  `status`,
  `workflow`,
  `device`,
  `member`,
  `repository`,
  `team`,
  `invite`,
  `notification`,
  `thread`,
  `attachment`,
  `list`,
]
export type EntityRefKind = (typeof entityRefKinds)[number]

export interface EntityRef {
  kind: EntityRefKind
  /** The row id; on a `list` ref the MEMBER kind. */
  id: string
  /** A human identifier (`EXP-42`) when the row has one. */
  identifier?: string
  /** The display string the result already carried (an issue's title, a
   *  board's name, a comment's body cut short); on a `list` ref the plural
   *  noun (`issues`). Absent when the result named only an id. */
  title?: string
  /** `list` refs only: how many rows the answer carried. */
  count?: number
}

export const PREVIEW_MAX_REFS = contract.expToolPreview.maxRefs
export const PREVIEW_TEXT_MAX = contract.expToolPreview.textMax

/** The bare tool name (`issues_create`) behind whatever namespace an adapter
 *  put in front (`mcp__exponential__exponential_issues_create`), or null when
 *  the name is not one of ours — the `expToolDisplay` rule. */
export function expToolShortName(toolName: string): string | null {
  const trimmed = toolName.trim()
  const prefix = contract.expToolDisplay.prefix
  for (const row of contract.expToolPreview.tools) {
    if (trimmed.length <= row.name.length) continue
    if (!trimmed.endsWith(row.name)) continue
    if (trimmed.slice(0, trimmed.length - row.name.length).endsWith(prefix)) {
      return row.name
    }
  }
  return null
}

/** The plural noun a `list` ref carries as its title — `kind + s`, with the
 *  one irregular. */
export function listRefNoun(memberKind: string): string {
  if (memberKind === `status`) return `statuses`
  if (memberKind === `repository`) return `repositories`
  return `${memberKind}s`
}

/** EXP-846: the JSON an MCP answer actually carries. MCP returns a
 *  `{content:[{type:"text",text:"…"}]}` envelope and the text is usually the
 *  JSON itself, so both shapes (and a bare value) resolve to one value here;
 *  anything unparseable is no payload. An `isError` envelope is never one. */
export function toolResultPayload(result: unknown): unknown {
  if (typeof result === `string`) {
    try {
      return JSON.parse(result) as unknown
    } catch {
      return undefined
    }
  }
  if (isRecord(result)) {
    if (result.isError === true) return undefined
    const content = result.content
    if (Array.isArray(content)) {
      for (const block of content) {
        if (!isRecord(block) || typeof block.text !== `string`) continue
        try {
          return JSON.parse(block.text) as unknown
        } catch {
          continue
        }
      }
      return undefined
    }
    return result
  }
  if (Array.isArray(result)) return result
  return undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === `object` && value !== null && !Array.isArray(value)
}

// Capped in UTF-16 code units, the unit the relay's zod counts, cut on a
// code point so no surrogate pair is split (the engine's `truncate_utf16`).
function clamp(text: string): string {
  let out = ``
  for (const ch of text.trim()) {
    if (out.length + ch.length > PREVIEW_TEXT_MAX) break
    out += ch
  }
  return out
}

/** The display string an entity row carries, in the order a row is known
 *  by: a title, a name, an action's snapshot name (a session), a repo's full
 *  name, a file's name, an invite's email, a device's label, a session's
 *  issue title, a comment's body (cut). */
const TITLE_KEYS = [
  `title`,
  `name`,
  `actionName`,
  `issueTitle`,
  `fullName`,
  `filename`,
  `email`,
  `label`,
  `body`,
] as const

/** A human issue identifier (`EXP-42`) — an input that names an issue by
 *  identifier rather than by uuid yields a ref carrying both. */
const IDENTIFIER_RE = /^[A-Z][A-Z0-9]*-\d+$/

function stringAt(row: Record<string, unknown>, key: string): string | null {
  const raw = row[key]
  if (typeof raw !== `string`) return null
  const trimmed = raw.trim()
  return trimmed ? trimmed : null
}

/** `kind@path` reads "the <kind> that node NAMES". On an object the id is
 *  its `<kind>Id` column first (an issue's `statusId`, a comment's `issueId`,
 *  a session's `issueId`; a member's `userId`), then its own `id` — so
 *  `issue@` over a comment row is the comment's issue and over an issue row
 *  the issue itself. A node that names ANOTHER row while carrying an `id` of
 *  its own (a comment, a session, a workflow node) lends that row nothing but
 *  the issue's `issueIdentifier`/`issueTitle` twins; a node with no own `id`
 *  (a `{issueId, identifier}` merge result, a `{deviceId, label}` device) is
 *  described BY its descriptors, which then belong to the named row. */
function namedIdKey(kind: EntityRefKind): string {
  return kind === `member` ? `userId` : `${kind}Id`
}

const ISSUE_TWINS = { identifier: `issueIdentifier`, title: `issueTitle` } as const

function firstString(
  row: Record<string, unknown>,
  keys: readonly string[]
): string | null {
  for (const key of keys) {
    const value = stringAt(row, key)
    if (value) return value
  }
  return null
}

/** One ref out of one node: an object naming the kind, or a bare id string
 *  (an input's `issueId`, which may be an identifier). */
function refFromNode(kind: EntityRefKind, node: unknown): EntityRef | null {
  if (typeof node === `string`) {
    const id = node.trim()
    if (!id) return null
    const ref: EntityRef = { kind, id: clamp(id) }
    if (kind === `issue` && IDENTIFIER_RE.test(id)) ref.identifier = ref.id
    return ref
  }
  if (!isRecord(node)) return null
  const ownId = stringAt(node, `id`)
  const namedKey = namedIdKey(kind)
  // A node that CARRIES the `<kind>Id` column names that row — or, when the
  // column is null, nothing at all (an action run's `issueId`, an issue with
  // no `statusId`): the fall-through to its own `id` is for rows that never
  // had the column.
  const named = namedKey in node ? stringAt(node, namedKey) : undefined
  const id = named === undefined ? ownId : named
  if (!id) return null
  const ref: EntityRef = { kind, id: clamp(id) }
  const foreign = named !== undefined && named !== ownId
  const identifierKeys: string[] = []
  const titleKeys: string[] = []
  if (kind === `issue`) {
    identifierKeys.push(ISSUE_TWINS.identifier)
    titleKeys.push(ISSUE_TWINS.title)
  }
  if (!foreign || ownId === null) {
    identifierKeys.push(`identifier`)
    titleKeys.push(...TITLE_KEYS)
  }
  let identifier = firstString(node, identifierKeys)
  // A workflow node (`{id, issueId, identifier, title}`) carries its issue's
  // identifier under the plain key; a foreign read takes it only when it
  // LOOKS like one, so a row's own identifier never lands on another kind.
  if (!identifier && kind === `issue` && foreign) {
    const plain = stringAt(node, `identifier`)
    if (plain && IDENTIFIER_RE.test(plain)) identifier = plain
  }
  if (identifier) ref.identifier = clamp(identifier)
  const title = firstString(node, titleKeys)
  if (title) ref.title = clamp(title)
  return ref
}

/** Walk a dotted path; a segment ending in `[]` fans out over an array. The
 *  empty path is the root itself; a leading `$` roots the walk in the tool's
 *  INPUT instead of its result (`issue@$issueIds[]`). */
function nodesAt(root: unknown, path: string): unknown[] {
  let nodes: unknown[] = [root]
  if (path === ``) return nodes
  for (const segment of path.split(`.`)) {
    const fan = segment.endsWith(`[]`)
    const key = fan ? segment.slice(0, -2) : segment
    const next: unknown[] = []
    for (const node of nodes) {
      const value = key === `` ? node : isRecord(node) ? node[key] : undefined
      if (value === undefined || value === null) continue
      if (fan) {
        if (Array.isArray(value)) next.push(...value)
      } else {
        next.push(value)
      }
    }
    nodes = next
  }
  return nodes
}

interface Term {
  kind: EntityRefKind
  /** `list:<member>` terms: the member kind. */
  member: EntityRefKind | null
  path: string
}

function parseTerm(term: string): Term | null {
  const at = term.indexOf(`@`)
  if (at < 0) return null
  const head = term.slice(0, at)
  const path = term.slice(at + 1)
  const kinds = entityRefKinds as readonly string[]
  if (head.startsWith(`list:`)) {
    const member = head.slice(`list:`.length)
    if (!kinds.includes(member)) return null
    return { kind: `list`, member: member as EntityRefKind, path }
  }
  if (!kinds.includes(head)) return null
  return { kind: head as EntityRefKind, member: null, path }
}

/** The contract spec for a tool, parsed. Null = not one of ours or nothing
 *  to preview. */
export function previewSpec(toolName: string): Term[] | null {
  const short = expToolShortName(toolName)
  if (!short) return null
  const row = contract.expToolPreview.tools.find((tool) => tool.name === short)
  if (!row || !row.refs.trim()) return null
  const terms = row.refs
    .split(/\s+/)
    .filter(Boolean)
    .map(parseTerm)
    .filter((term): term is Term => term !== null)
  return terms.length > 0 ? terms : null
}

/** THE rule: the refs a tool's answer yields, in spec order, deduplicated
 *  per kind by id AND identifier (an input's `EXP-12` next to the row it
 *  resolved to is one issue), capped at `maxRefs`. `[]` for an error result,
 *  an unreadable payload, an unknown tool or a tool whose spec is empty.
 *  `input` is the call's own arguments, for the tools whose answer names no
 *  row (`pr_open`, the relation and label links). */
export function toolResultPreview(
  toolName: string,
  result: unknown,
  input?: unknown
): EntityRef[] {
  const terms = previewSpec(toolName)
  if (!terms) return []
  const payload = toolResultPayload(result)
  if (payload === undefined || payload === null) return []
  const refs: EntityRef[] = []
  const seen = new Set<string>()
  const push = (ref: EntityRef | null) => {
    if (!ref || refs.length >= PREVIEW_MAX_REFS) return
    const keys = [`${ref.kind}:${ref.id}`]
    if (ref.identifier) keys.push(`${ref.kind}:${ref.identifier}`)
    if (keys.some((key) => seen.has(key))) return
    for (const key of keys) seen.add(key)
    refs.push(ref)
  }
  const rootOf = (path: string): [unknown, string] =>
    path.startsWith(`$`) ? [input, path.slice(1)] : [payload, path]
  for (const term of terms) {
    const [root, path] = rootOf(term.path)
    if (term.kind === `list` && term.member) {
      const [node] = nodesAt(root, path)
      if (!Array.isArray(node)) continue
      const total = isRecord(root) ? root.total : undefined
      const count =
        typeof total === `number` && Number.isFinite(total) && total >= 0
          ? Math.round(total)
          : node.length
      push({
        kind: `list`,
        id: term.member,
        title: listRefNoun(term.member),
        count,
      })
      for (const member of node) push(refFromNode(term.member, member))
      continue
    }
    for (const node of nodesAt(root, path)) push(refFromNode(term.kind, node))
  }
  return refs
}
