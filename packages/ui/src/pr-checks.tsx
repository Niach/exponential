import type * as React from "react"
import prChecksFixture from "@exp/domain-contract/fixtures/pr-checks.json"
import mergeTrainFixture from "@exp/domain-contract/fixtures/merge-train.json"
import {
  checksList,
  checksSummary,
  PR_CHECKS_COPY,
  PR_CHECKS_TEST_IDS,
  type CheckGlyph,
  type CheckTone,
  type PrCheckRowLike,
} from "@exp/domain-contract/pr-checks"
import { statusPill, type PillGlyph, type StatusPillInput, type StatusPillView } from "@exp/domain-contract/merge-train"

import { cn } from "./cn"
import { Spinner } from "./feedback"
import { conceptIcon } from "./icons.generated"
import { MenuPanel, type MenuEntry, type MenuSubmenuEntry } from "./menu"
import { Pill } from "./pill"

// #EXP-1611 (fixtures/pr-checks.json CHECKS LIST): the PR's CI on the chat
// header ×4, the `…` menu's Checks submenu over the synced
// `pull_request_checks` rows. #EXP-1710 (merge-train.json STATUS PILL): THE
// header pill is the merge STATUS pill (`statusPill`); its tap opens the same
// Checks list. The web app (session-merge-button.tsx, chat-guide.tsx) owns
// the data and the confirms, this file only the faces.

/** A check tone's glyph tint. */
export const PR_CHECK_TONE_CLASS: Record<CheckTone, string> = {
  muted: `text-muted-foreground`,
  green: `text-emerald-400`,
  red: `text-destructive`,
}

const GLYPHS: Record<Exclude<CheckGlyph, `ci-running`>, ReturnType<typeof conceptIcon>> = {
  "ci-queued": conceptIcon(`ci-queued`),
  "ci-passed": conceptIcon(`ci-passed`),
  "ci-failed": conceptIcon(`ci-failed`),
  "ci-skipped": conceptIcon(`ci-skipped`),
}

const PILL_GLYPHS: Record<Exclude<PillGlyph, `spinner`>, ReturnType<typeof conceptIcon>> = {
  "pr-draft": conceptIcon(`pr-draft`),
  "ci-queued": conceptIcon(`ci-queued`),
  "pr-merged": conceptIcon(`pr-merged`),
  "pr-conflict": conceptIcon(`pr-conflict`),
  "ci-failed": conceptIcon(`ci-failed`),
  "pr-hold": conceptIcon(`pr-hold`),
}

/** A check's status glyph tinted by its tone; `ci-running` = THE Spinner
 *  at the glyph slot's 16px (menu icon size, both densities; the Spinner's
 *  own default is the 10px inline mark and it pins an inline size). */
export function PrCheckGlyph({
  glyph,
  tone,
  className,
}: {
  glyph: CheckGlyph
  tone: CheckTone
  className?: string
}) {
  if (glyph === `ci-running`) {
    return <Spinner aria-hidden size={16} className={cn(PR_CHECK_TONE_CLASS[tone], className)} />
  }
  const Glyph = GLYPHS[glyph]
  return <Glyph aria-hidden data-glyph={glyph} className={cn(PR_CHECK_TONE_CLASS[tone], className)} />
}

/** The `…` menu's leading Checks submenu (`checksSummary` + `checksList`);
 *  null = no rows (no entry). A pick opens the check's logs. */
export function prChecksMenuEntry(
  checks: readonly PrCheckRowLike[],
  now: Date | string,
  onOpen: (url: string) => void = (url) => {
    window.open(url, `_blank`, `noopener`)
  }
): MenuSubmenuEntry | null {
  const summary = checksSummary(checks)
  if (!summary) return null
  return {
    kind: `submenu`,
    id: `checks`,
    "data-testid": PR_CHECKS_TEST_IDS.checksMenu,
    icon: <PrCheckGlyph glyph={summary.glyph} tone={summary.tone} />,
    label: PR_CHECKS_COPY.checks,
    value: summary.value,
    contentClassName: `w-64`,
    entries: checksList(checks, now).map(
      (row, index): MenuEntry => ({
        kind: `item`,
        id: `check-${index}-${row.name}`,
        "data-testid": PR_CHECKS_TEST_IDS.checkRow,
        icon: <PrCheckGlyph glyph={row.glyph} tone={row.tone} />,
        label: row.name,
        value: row.duration ?? undefined,
        disabled: row.url === null,
        onSelect: () => {
          if (row.url) onOpen(row.url)
        },
      })
    ),
  }
}

/** THE header merge STATUS pill's face over `statusPill` (merge-train.json):
 *  THE Pill md, mode action (readonly when `tap` is null), primary only for
 *  `Needs approval`; the glyph by concept, `spinner` = THE Spinner, tone red
 *  tints the glyph only. `busy` = the approval in flight (its own spinner). */
export function PrStatusPill({
  view,
  busy = false,
  className,
  ...props
}: Omit<React.ComponentProps<`button`>, `children`> & {
  view: StatusPillView
  busy?: boolean
}) {
  const tone = view.tone === `red` ? PR_CHECK_TONE_CLASS.red : undefined
  const Glyph = view.glyph === `spinner` ? null : PILL_GLYPHS[view.glyph]
  const glyph =
    busy || !Glyph ? (
      <Spinner aria-hidden size={16} className="text-current" />
    ) : (
      <Glyph aria-hidden data-glyph={view.glyph} className={tone} />
    )
  return (
    <Pill
      size="md"
      mode={view.tap === null ? `readonly` : `action`}
      primary={view.primary}
      data-glyph={view.glyph}
      data-tone={view.tone}
      className={cn(`shrink-0`, className)}
      {...props}
    >
      {glyph}
      {view.label}
    </Pill>
  )
}

/** #EXP-1710 (DEV-ONLY view `pr-checks`, the styleguide entry): every
 *  status pill of merge-train.json `statusPill` (case for case), then the
 *  open Checks submenu as a static panel over pr-checks.json `specimen`. */
export function PrChecksSpecimen() {
  const { checks, now } = prChecksFixture.specimen
  const entry = prChecksMenuEntry(checks, now, () => {})
  return (
    <div data-testid="pr-checks-specimen" className="flex max-w-full flex-col items-start gap-6">
      <div className="flex max-w-xl flex-wrap items-center gap-2">
        {mergeTrainFixture.statusPill.map((c) => (
          <PrStatusPill
            key={c.name}
            view={statusPill(c.input as StatusPillInput)}
            data-case={c.name}
            tabIndex={-1}
          />
        ))}
      </div>
      {entry && (
        <div className="flex flex-wrap items-start gap-1">
          <MenuPanel entries={[entry]} className="w-64" />
          <MenuPanel entries={entry.entries ?? []} className="w-64" />
        </div>
      )}
    </div>
  )
}
