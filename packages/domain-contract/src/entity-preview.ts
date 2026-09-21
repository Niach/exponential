// EXP-920: the ONE entity-preview rule. An Exponential MCP tool's settled
// row carries `preview.refs` (contract `entityRefKind`, distilled per
// `expToolPreview.tools` by web `lib/mcp/preview.ts` and the engine's
// `mapper.rs`). Every client renders those refs the SAME way: one CHIP per
// ref (a glyph + a short label), a hover card / popover resolved from the
// client's own synced rows, and a tap that opens the entity's detail page.
//
// This module owns what is pure and platform-free about that:
// - `ENTITY_REF_ICON`: the icon CONCEPT each kind draws (an issue paints its
//   resolved status glyph instead once the row is synced);
// - `entityKindNoun`: the product noun, singular and plural ("run", not
//   "session");
// - `entityChipLabel`: the chip's text, clamped to `CHIP_LABEL_MAX`;
// - `groupPreviewRefs`: how a `list` ref absorbs the member refs behind it
//   into ONE chip whose card lists them.
//
// Hand-mirrored ×4 (web via this module, desktop `domain::entity_preview`,
// iOS `ExpCore/Sources/Domain/EntityPreview.swift`, Android
// `domain/EntityPreview.kt`) and byte-locked by `fixtures/entity-chip.json`.

import contractJson from "../contract.json" with { type: "json" }

const json = contractJson as unknown as {
  entityRefKind: { values: readonly string[] }
  expToolPreview: { maxRefs: number; textMax: number }
}

export const ENTITY_REF_KINDS: readonly string[] = json.entityRefKind.values

/** One entity an Exponential tool's answer named — the wire shape
 *  (`tool_update.preview.refs[]`), every string already clamped by the
 *  publisher to `expToolPreview.textMax`. */
export interface EntityRefLike {
  kind: string
  id: string
  identifier?: string
  title?: string
  count?: number
}

/** A chip's label never runs past this many characters (code points); a
 *  longer one is cut to `CHIP_LABEL_MAX - 1` and ends in an ellipsis. */
export const CHIP_LABEL_MAX = 48

/** The icon CONCEPT a kind's chip and card header draw (`packages/icons`
 *  `semantic`). A `list` chip draws its MEMBER kind's icon. */
export const ENTITY_REF_ICON: Readonly<Record<string, string>> = {
  issue: `ui-issue`,
  board: `nav-boards`,
  action: `nav-actions`,
  automation: `nav-automations`,
  comment: `notification-issue-comment`,
  session: `coding-running`,
  label: `settings-labels`,
  status: `settings-statuses`,
  workflow: `nav-workflows`,
  device: `ui-device`,
  member: `ui-avatar-placeholder`,
  repository: `ui-repository`,
  team: `ui-team`,
  invite: `ui-invite`,
  notification: `nav-notifications`,
  thread: `nav-support`,
  attachment: `ui-attach`,
  list: `ui-checklist`,
}

const NOUNS: Readonly<Record<string, readonly [string, string]>> = {
  issue: [`issue`, `issues`],
  board: [`board`, `boards`],
  action: [`action`, `actions`],
  automation: [`automation`, `automations`],
  comment: [`comment`, `comments`],
  session: [`run`, `runs`],
  label: [`label`, `labels`],
  status: [`status`, `statuses`],
  workflow: [`workflow`, `workflows`],
  device: [`device`, `devices`],
  member: [`member`, `members`],
  repository: [`repository`, `repositories`],
  team: [`team`, `teams`],
  invite: [`invite`, `invites`],
  notification: [`notification`, `notifications`],
  thread: [`thread`, `threads`],
  attachment: [`attachment`, `attachments`],
  list: [`list`, `lists`],
}

/** The product noun for a kind: singular for `count` 1, plural otherwise.
 *  An unknown kind (a NEWER publisher) reads as `item`/`items`. */
export function entityKindNoun(kind: string, count = 1): string {
  const pair = NOUNS[kind] ?? [`item`, `items`]
  return count === 1 ? pair[0] : pair[1]
}

/** The icon concept a ref draws: a `list` ref its member kind's, anything
 *  else its own; an unknown kind falls back to the `list` glyph. */
export function entityRefIcon(ref: EntityRefLike): string {
  const kind = ref.kind === `list` ? ref.id : ref.kind
  return ENTITY_REF_ICON[kind] ?? ENTITY_REF_ICON.list
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1)
}

/** Cut to `CHIP_LABEL_MAX` code points, ellipsis included in the budget;
 *  the kept prefix loses its trailing whitespace so no label ends in ` …`. */
export function clampChipLabel(text: string): string {
  const points = Array.from(text.trim())
  if (points.length <= CHIP_LABEL_MAX) return points.join(``)
  return `${points.slice(0, CHIP_LABEL_MAX - 1).join(``).trimEnd()}…`
}

/** The chip's text:
 *  - a `list`: `<count> <member noun>` ("3 issues", "1 run", "0 labels");
 *  - an issue: its identifier, else its title, else "Issue";
 *  - anything else: its title, else the capitalized noun ("Board", "Run").
 *  Whitespace-only titles count as absent. */
export function entityChipLabel(ref: EntityRefLike): string {
  if (ref.kind === `list`) {
    const count = Math.max(0, Math.round(ref.count ?? 0))
    return clampChipLabel(`${count} ${entityKindNoun(ref.id, count)}`)
  }
  const identifier = ref.identifier?.trim()
  const title = ref.title?.trim()
  if (ref.kind === `issue` && identifier) return clampChipLabel(identifier)
  if (title) return clampChipLabel(title)
  return capitalize(entityKindNoun(ref.kind, 1))
}

/** The secondary text an issue chip shows beside its identifier (the
 *  title), null for every other kind or when the identifier already IS the
 *  label. */
export function entityChipDetail(ref: EntityRefLike): string | null {
  if (ref.kind !== `issue`) return null
  const identifier = ref.identifier?.trim()
  const title = ref.title?.trim()
  return identifier && title ? clampChipLabel(title) : null
}

export interface PreviewRefGroup<T extends EntityRefLike = EntityRefLike> {
  ref: T
  /** The member refs a `list` chip's card lists; `[]` on every other chip. */
  members: T[]
}

/** One chip per group: a `list` ref opens a group and absorbs every
 *  DIRECTLY following ref of its member kind; the first ref of another kind
 *  (or another list) closes it. Any other ref is a group of its own. */
export function groupPreviewRefs<T extends EntityRefLike>(
  refs: readonly T[]
): PreviewRefGroup<T>[] {
  const groups: PreviewRefGroup<T>[] = []
  let open: PreviewRefGroup<T> | null = null
  for (const ref of refs) {
    if (open && ref.kind !== `list` && ref.kind === open.ref.id) {
      open.members.push(ref)
      continue
    }
    const group: PreviewRefGroup<T> = { ref, members: [] }
    groups.push(group)
    open = ref.kind === `list` ? group : null
  }
  return groups
}
