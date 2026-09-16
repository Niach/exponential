import {
  additionsLabel,
  deletionsLabel,
  type DiffStatus,
} from "@exp/domain-contract/diff"

import { cn } from "./cn"
import { middleTruncate } from "./truncate"

// EXP-895 — the three atoms every diff surface is built from: the ±counts, the
// one-letter status and the dimmed-directory path. All three take their colours
// from the shared `--diff-*` tokens (`text-diff-add-fg` / `text-diff-del-fg`),
// never from a raw tailwind palette class, so the natives can mirror them off
// `packages/design-tokens/tokens.json` instead of guessing an emerald.
//
// The deletion count is U+2212 MINUS SIGN, not a hyphen — `deletionsLabel` in
// the contract owns that decision, so a client that spells it `-3` drifts.

export function DiffCounts({
  additions,
  deletions,
  className,
}: {
  additions: number
  deletions: number
  className?: string
}) {
  return (
    <span className={cn(`shrink-0 font-mono`, className)}>
      <span className="text-diff-add-fg">{additionsLabel(additions)}</span>
      {` `}
      <span className="text-diff-del-fg">{deletionsLabel(deletions)}</span>
    </span>
  )
}

const STATUS_LETTER: Record<DiffStatus, string> = {
  added: `A`,
  removed: `D`,
  modified: `M`,
  renamed: `R`,
  copied: `C`,
}

/**
 * The status letter a file list leads with. Only the two states that ARE a
 * colour in the diff body carry one: `A` the addition green, `D` the deletion
 * red. `M`/`R`/`C` stay muted — an amber "modified" and a sky "renamed" (the
 * pre-EXP-895 palette) invented two accent hues the token set does not have.
 */
export function DiffStatusLetter({
  status,
  className,
}: {
  status: DiffStatus
  className?: string
}) {
  return (
    <span
      className={cn(
        `w-3 shrink-0 text-center font-mono font-semibold`,
        status === `added`
          ? `text-diff-add-fg`
          : status === `removed`
            ? `text-diff-del-fg`
            : `text-muted-foreground`,
        className
      )}
      data-status={status}
    >
      {STATUS_LETTER[status]}
    </span>
  )
}

// EXP-698: on a phone a trailing ellipsis eats the only part of a path that
// identifies the file. The DIRECTORY gives way instead — middle-truncated, so
// both ends of it stay readable — and the filename is never cut. `isMobile` is
// threaded in rather than read here: a 200-file diff would otherwise register
// 200 matchMedia listeners, one per path label.
const MOBILE_DIR_CHARS = 22

/** `apps/web/src/` dimmed, `file.tsx` at full weight — one mono run. */
export function DiffPath({
  path,
  isMobile = false,
  className,
}: {
  path: string
  isMobile?: boolean
  className?: string
}) {
  const slash = path.lastIndexOf(`/`)
  const dir = slash >= 0 ? path.slice(0, slash + 1) : ``
  const base = path.slice(slash + 1)
  return (
    <span className={cn(`min-w-0 truncate font-mono`, className)}>
      {dir ? (
        <span className="text-muted-foreground">
          {isMobile ? middleTruncate(dir, MOBILE_DIR_CHARS) : dir}
        </span>
      ) : null}
      {base}
    </span>
  )
}
