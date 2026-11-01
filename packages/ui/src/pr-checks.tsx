import type * as React from "react"
import prChecksFixture from "@exp/domain-contract/fixtures/pr-checks.json"
import {
  checksList,
  checksSummary,
  mergePill,
  PR_CHECKS_COPY,
  PR_CHECKS_TEST_IDS,
  type CheckGlyph,
  type CheckTone,
  type MergePillView,
  type PrCheckRowLike,
} from "@exp/domain-contract/pr-checks"

import { cn } from "./cn"
import { Spinner } from "./feedback"
import { conceptIcon } from "./icons.generated"
import { MenuPanel, type MenuEntry, type MenuSubmenuEntry } from "./menu"
import { Pill } from "./pill"

// #EXP-1611 (fixtures/pr-checks.json MERGE PILL / CHECKS LIST): the PR's CI
// on the thread header ×4. THE Merge pill in its check states and the `…`
// menu's Checks submenu, both over the synced `pull_request_checks` rows;
// the web app (session-merge-button.tsx, thread-guide.tsx) owns the data and
// the confirms, this file only the faces.

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

const PrMergedIcon = conceptIcon(`pr-merged`)

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

/** THE header Merge pill's face over `mergePill` (the web app wires the
 *  confirms): primary + pr-merged | primary + THE Spinner (pending) | plain
 *  + the red ci-failed glyph `Checks failing`. `merging` = the merge in
 *  flight (its own spinner). */
export function PrMergePill({
  view,
  merging = false,
  className,
  ...props
}: Omit<React.ComponentProps<`button`>, `children`> & {
  view: MergePillView
  merging?: boolean
}) {
  const glyph = merging ? (
    <Spinner aria-hidden />
  ) : view.glyph === `spinner` ? (
    <Spinner aria-hidden size={16} className="text-current" />
  ) : view.glyph === `ci-failed` ? (
    <PrCheckGlyph glyph="ci-failed" tone="red" />
  ) : (
    <PrMergedIcon aria-hidden />
  )
  return (
    <Pill
      size="md"
      mode="action"
      primary={view.primary}
      data-checks={view.confirm === `failing` ? `failure` : view.glyph === `spinner` ? `pending` : undefined}
      className={cn(`shrink-0`, className)}
      {...props}
    >
      {glyph}
      {view.label}
    </Pill>
  )
}

const SPECIMEN_STATES = [`none`, `pending`, `success`, `failure`] as const

/** #EXP-1611 (DEV-ONLY view `pr-checks`, the styleguide entry): the four
 *  Merge pills (none, pending, success, failure), then the open Checks
 *  submenu as a static panel over the fixture's `specimen`. */
export function PrChecksSpecimen() {
  const { checks, now } = prChecksFixture.specimen
  const entry = prChecksMenuEntry(checks, now, () => {})
  return (
    <div data-testid="pr-checks-specimen" className="flex max-w-full flex-col items-start gap-6">
      <div className="flex flex-wrap items-center gap-2">
        {SPECIMEN_STATES.map((state) => (
          <PrMergePill key={state} view={mergePill({ state, stack: false })} data-state={state} />
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
