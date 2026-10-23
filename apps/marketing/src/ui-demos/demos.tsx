/* ─── The docs' UI islands — the demo map (EXP-887) ───

   Every entry is ONE real `@exp/ui` component tree. `scripts/ui-demos.tsx`
   renders each to static markup inside a declarative shadow root and compiles
   the package stylesheet with THIS directory as the Tailwind scan base — so
   the wrapper classes written here are emitted alongside the components' own.

   Why a shadow root: marketing has no Tailwind and hand-writes tokens of the
   same names as the web theme (`--border`, `--input`, `--accent`), so the web
   CSS must never reach this document. It rides inside the island instead.

   Keep every demo a RESTING state: `renderToStaticMarkup` has no portals, no
   hover and no open menus, and the handler below exists only so the removable
   variant renders its ✕ (see `island.ts` for the full list of limits).

   Statuses come from the package's own resolution table rather than literal
   icon names and colour classes, so a docs chip can never drift from the one
   the four clients draw. */

import type { ReactElement } from "react"
import {
  BUILTIN_STATUS_COLOR_CLASS,
  IssueChip,
  categoryStatusIcon,
  type StatusGlyphProps,
} from "@exp/ui"

/** The builtin In Progress / In Review pair — the 2/4 and 3/4 pie clocks. */
const IN_PROGRESS: StatusGlyphProps = {
  icon: categoryStatusIcon(`started`, 0, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_progress,
}
const IN_REVIEW: StatusGlyphProps = {
  icon: categoryStatusIcon(`started`, 1, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_review,
}

/** contract.json `composerUi.implementHeadline` (×4). Marketing does not
 *  declare @exp/domain-contract, so the word is copied, not imported. */
const IMPLEMENT_HEADLINE = `Implement`

/** Static markup: nothing here is ever clicked. */
const noop = () => {}

export const UI_DEMOS_SOURCE: Record<string, () => ReactElement> = {
  /* /docs/issues/ — what a `#EXP-885` reference becomes once it resolves. */
  "issue-chip": () => (
    <div className="flex flex-wrap items-center gap-2">
      <IssueChip
        identifier="EXP-885"
        title="One issue chip, every surface"
        status={IN_REVIEW}
      />
      <IssueChip
        identifier="EXP-887"
        title="Real components in the docs"
        status={IN_PROGRESS}
      />
    </div>
  ),

  /* /docs/coding/ — the Agent composer's HEADLINE with an issue subject
     (apps/web launch-dialog/launch-headline.tsx + subject-chips.tsx): the
     contract's `composerUi.implementHeadline` verb, then one removable
     IssueChip per issue (`max-w-[20rem]`, the ✕ the only thing that drops
     one). A prompt or an agent subject REPLACES the issue chips with one
     chip of its own; the two never share the row, so the demo shows one
     subject kind only. */
  "subject-chips": () => (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5 px-1 text-lg leading-tight font-semibold">
      <span className="shrink-0">{IMPLEMENT_HEADLINE}</span>
      <IssueChip
        identifier="EXP-885"
        title="One issue chip, every surface"
        status={IN_REVIEW}
        className="max-w-[20rem]"
        onRemove={noop}
      />
      <IssueChip
        identifier="EXP-887"
        title="Real components in the docs"
        status={IN_PROGRESS}
        className="max-w-[20rem]"
        onRemove={noop}
      />
    </div>
  ),
}
