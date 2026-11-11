import {
  additionsLabel,
  deletionsLabel,
  type DiffStatus,
} from "@exp/domain-contract/diff"

import { watchMiddleTruncate } from "@exponential_app/eui/web"
import { useLayoutEffect, useRef, useState } from "react"

import { cn } from "./cn"

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
  title,
  "aria-label": ariaLabel,
}: {
  additions: number
  deletions: number
  className?: string
  /** EXP-1152: the counts as one string (`+12 −2`) when they ARE a control's
   *  label. */
  title?: string
  "aria-label"?: string
}) {
  return (
    <span
      className={cn(`shrink-0 font-mono`, className)}
      title={title}
      aria-label={ariaLabel}
    >
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

// EXP-698 + #EXP-1789: a trailing ellipsis eats the only part of a path that
// identifies the file. The DIRECTORY gives way instead — middle-cut to the
// width it gets (the genUI Text `truncate: "middle"` rule, `watchMiddleTruncate`,
// so both ends of it stay readable) — and the filename is never cut unless it
// alone outgrows the row. The full path is the hover title.
//
// Two layers, so the shown line never feeds back into the room it gets: an
// INVISIBLE sizer row claims the room — the whole directory and the whole
// name as `::before` copies (never in `textContent`), the directory shrinking
// first (down to its `…`), the name only after it — and `watchMiddleTruncate`
// cuts the directory to its sizer's width. The VISIBLE row lies over it from
// the left edge: the cut directory hugs the name, any slack the cut leaves
// falls after the name, and the name end-cuts only once the directory is down
// to its `…`.

/** `apps/web/src/` dimmed, `file.tsx` at full weight — one mono run. */
export function DiffPath({
  path,
  className,
}: {
  path: string
  className?: string
}) {
  const slash = path.lastIndexOf(`/`)
  const dir = slash >= 0 ? path.slice(0, slash + 1) : ``
  const base = path.slice(slash + 1)
  const dirRef = useRef<HTMLSpanElement>(null)
  const [shown, setShown] = useState<string | null>(null)
  useLayoutEffect(() => {
    const el = dirRef.current
    if (!dir || !el) return
    return watchMiddleTruncate(el, dir, setShown)
  }, [dir])
  return (
    <span
      className={cn(`relative flex min-w-0 overflow-hidden font-mono`, className)}
      title={path}
      data-slot="diff-path"
    >
      {dir ? (
        <span
          ref={dirRef}
          aria-hidden
          data-full={dir}
          data-slot="diff-path-dir-sizer"
          className="invisible min-w-[2ch] shrink-[1000] overflow-hidden whitespace-pre before:content-[attr(data-full)]"
        />
      ) : null}
      <span
        aria-hidden
        data-full={base}
        className="invisible min-w-0 overflow-hidden whitespace-pre before:content-[attr(data-full)]"
      />
      <span className="absolute inset-0 flex min-w-0">
        {dir ? (
          <span className="shrink-0 whitespace-pre text-muted-foreground">
            {shown ?? dir}
          </span>
        ) : null}
        <span className="min-w-0 truncate">{base}</span>
      </span>
    </span>
  )
}
