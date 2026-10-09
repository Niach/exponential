import fixture from "@exp/domain-contract/fixtures/composer-menu.json"

// EXP-1249: the composer's ONE "+" menu as a pure layout, replayed ×4 off
// `packages/domain-contract/fixtures/composer-menu.json` (desktop
// chat_screen.rs, iOS AgentComposerCard.swift, Android AgentComposer.kt):
// which rows show for the picked agent / device / subject, in what order,
// with which concept glyph and copy. The React half (`composer-plus-menu.tsx`)
// only binds each row id to the model.

export type ComposerMenuRowId =
  | `implement-issue`
  | `run-action`
  | `add-file`
  | `effort`
  | `subagents`
  | `ultracode`
  | `mcp-servers`
  | `computer-use`

export type ComposerMenuCondition = keyof typeof fixture.conditions

export type ComposerMenuConditions = Record<ComposerMenuCondition, boolean>

export interface ComposerMenuRow {
  id: ComposerMenuRowId
  kind: `submenu` | `item` | `toggle`
  label: string
  /** The codex wording of the row, when it has one ("Reasoning"). */
  codexLabel?: string
  /** A concept id (`packages/icons/icons.json`). */
  icon: string
}

export type ComposerMenuLayoutEntry = ComposerMenuRow | { kind: `separator` }

interface FixtureRow {
  id?: string
  kind: string
  label?: string
  codexLabel?: string
  icon?: string
  when?: string
}

const ROWS = fixture.rows as readonly FixtureRow[]

export const COMPOSER_MENU_TEST_IDS = fixture.testIds
export const COMPOSER_PLUS_LABEL = fixture.plusLabel
/** The capability a device advertises once it reads the per-run flag. */
export const COMPUTER_USE_RUN_CAP = fixture.computerUse.cap
/** How many chat suggestions the quiet rows under the composer show. */
export const COMPOSER_SUGGESTION_COUNT = fixture.suggestions.count
export const COMPOSER_SUGGESTION_ICON = fixture.suggestions.icon

export function composerMenuRowTestId(id: ComposerMenuRowId): string {
  return fixture.testIds.row.replace(`{id}`, id)
}

/** The rows the "+" shows under `conditions`, separators tidied (never
 *  leading, trailing or doubled). */
export function composerMenuLayout(
  conditions: ComposerMenuConditions
): ComposerMenuLayoutEntry[] {
  const out: ComposerMenuLayoutEntry[] = []
  for (const row of ROWS) {
    if (row.kind === `separator`) {
      const last = out[out.length - 1]
      if (last && last.kind !== `separator`) out.push({ kind: `separator` })
      continue
    }
    if (row.when && !conditions[row.when as ComposerMenuCondition]) continue
    out.push({
      id: row.id as ComposerMenuRowId,
      kind: row.kind as ComposerMenuRow[`kind`],
      label: row.label ?? ``,
      ...(row.codexLabel ? { codexLabel: row.codexLabel } : {}),
      icon: row.icon ?? ``,
    })
  }
  while (out[out.length - 1]?.kind === `separator`) out.pop()
  return out
}

/** The Implement submenu's footer button once something is picked. */
export function implementButtonLabel(count: number): string {
  return count === 1
    ? fixture.implementButton.one
    : fixture.implementButton.many.replace(`{n}`, String(count))
}
