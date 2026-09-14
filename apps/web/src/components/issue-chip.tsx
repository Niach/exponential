import type { MouseEvent } from "react"
import { IssuePreviewHoverCard } from "@/components/issue-preview-card"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { conceptIcon } from "@/lib/icons.generated"
import type { StatusResolvable } from "@/lib/team-statuses"
import { cn } from "@/lib/utils"

// EXP-885 — THE issue chip. ONE component for every surface that names an
// issue as a chip: the steering feed's prose (`IssueRefText`), the MCP tool
// result preview, the timeline's relation events and the launch composer's
// subject chips. Inside a markdown editor the same chip is a ProseMirror
// DECORATION instead (lib/issue-ref-extension.ts), because the document text
// has to stay the bare `#IDENT` token — the two share their box through the
// `issue-chip` class in styles.css, and issue-chip.test.tsx locks that pair.
//
// The style, byte-for-byte across all four clients: a small rounded RECT
// (6px, never a capsule), `--border` hairline over `--accent`, then status
// glyph · mono muted identifier · sans/500 foreground title, truncated.
//
// A chip renders only for an issue the caller already resolved; an unresolved
// token stays prose. `onClick` opens it, `onRemove` adds the trailing ✕ that
// composers need — the ✕ is the ONLY part of a chip that removes it.

const UiCloseIcon = conceptIcon(`ui-close`)

/** What a chip needs of an issue: its identity plus what resolves a status. */
export type IssueChipIssue = StatusResolvable & {
  id: string
  identifier: string
  title: string
}

export function IssueChip({
  issue,
  onClick,
  onRemove,
  removeLabel,
  removeDisabled,
  preview = true,
  className,
  testId,
  removeTestId,
}: {
  issue: IssueChipIssue
  /** Open the issue. Omitted = an inert chip (no target, no pointer). */
  onClick?: () => void
  /** Composers only: render the trailing ✕. */
  onRemove?: () => void
  /** The ✕'s accessible name; defaults to `Remove <IDENT>`. */
  removeLabel?: string
  /** Keep the ✕ in place but inert (a submitting composer). */
  removeDisabled?: boolean
  /** Opt out of the hover preview (composers, tight rows). */
  preview?: boolean
  className?: string
  testId?: string
  removeTestId?: string
}) {
  const tooltip = `${issue.identifier} · ${issue.title}`
  const body = (
    <>
      {/* 0.875em of the chip's own 0.75rem — the exact glyph box the editor
          decoration paints with its ::before mask. */}
      <IssueStatusIcon
        issue={issue}
        className="!h-[0.875em] !w-[0.875em] shrink-0"
      />
      <span className="shrink-0 font-mono text-muted-foreground">
        {issue.identifier}
      </span>
      <span className="min-w-0 truncate text-[0.8125rem] font-medium text-foreground">
        {issue.title}
      </span>
    </>
  )

  const chip = (
    <span
      data-slot="issue-chip"
      data-testid={testId}
      data-removable={onRemove ? `true` : undefined}
      title={tooltip}
      // `issue-chip` is the shared box (styles.css); everything here is
      // LAYOUT. `align-middle`: the chip flows inline in prose, and an
      // inline-flex box would otherwise sit on the text baseline.
      className={cn(
        `issue-chip inline-flex max-w-[18rem] items-center gap-1 align-middle text-xs`,
        className
      )}
    >
      {onClick ? (
        <button
          type="button"
          onClick={onClick}
          aria-label={`Open ${issue.identifier}`}
          className="flex min-w-0 cursor-pointer items-center gap-1 rounded-[4px] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
        >
          {body}
        </button>
      ) : (
        body
      )}
      {onRemove && (
        <button
          type="button"
          aria-label={removeLabel ?? `Remove ${issue.identifier}`}
          title={removeLabel ?? `Remove ${issue.identifier}`}
          data-testid={removeTestId}
          disabled={removeDisabled}
          onClick={(event: MouseEvent) => {
            event.stopPropagation()
            onRemove()
          }}
          className="flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-[4px] text-muted-foreground outline-none hover:bg-glass-active hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50"
        >
          <UiCloseIcon className="size-3" />
        </button>
      )}
    </span>
  )

  if (!preview) return chip
  return <IssuePreviewHoverCard issueId={issue.id}>{chip}</IssuePreviewHoverCard>
}
