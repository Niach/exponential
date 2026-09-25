import type { ReactElement } from "react"
import { IssueChip as IssueChipView, type IssueChipLinkProps } from "@exp/ui"
import { IssuePreviewHoverCard } from "@/components/issue-preview-card"
import { issueMenuProps } from "@/components/issue-context-menu/attr"
import {
  statusColorClass,
} from "@/components/issue-properties/status-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusResolvable } from "@/lib/team-statuses"

// EXP-885 — THE issue chip, DATA half. The box, the three parts and the ✕ are
// `IssueChip` in @exp/ui; this file is what only the app can do: resolve the
// issue's status against the team's synced rows (`useTeamStatuses`) and wrap
// the chip in its hover preview.
//
// Inside a markdown editor the same chip is a ProseMirror DECORATION instead
// (lib/issue-ref-extension.ts), because the document text has to stay the bare
// `#IDENT` token — the two share their box through the `issue-chip` class in
// the package's styles.css, and issue-chip.test.tsx locks that pair.
//
// A chip renders only for an issue the caller already resolved; an unresolved
// token stays prose. `onClick` opens it, `onRemove` adds the trailing ✕ that
// composers need — the ✕ is the ONLY part of a chip that removes it.

/** What a chip needs of an issue: its identity plus what resolves a status. */
export type IssueChipIssue = StatusResolvable & {
  id: string
  identifier: string
  title: string
}

export function IssueChip({
  issue,
  onClick,
  link,
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
  /** Open the issue as a real router `<Link>` (⌘-click, copy address,
   *  preloading) — the surfaces that jump to the issue's page. */
  link?: (props: IssueChipLinkProps) => ReactElement
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
  const { resolve } = useTeamStatusesContext()
  const status = resolve(issue)

  const chip = (
    <IssueChipView
      {...issueMenuProps(issue.id)}
      identifier={issue.identifier}
      title={issue.title}
      status={{
        icon: status.icon,
        colorClass: statusColorClass(status),
        colorHex: status.builtinKey ? undefined : status.colorHex,
      }}
      onClick={onClick}
      link={link}
      onRemove={onRemove}
      removeLabel={removeLabel}
      removeDisabled={removeDisabled}
      className={className}
      testId={testId}
      removeTestId={removeTestId}
    />
  )

  if (!preview) return chip
  return <IssuePreviewHoverCard issueId={issue.id}>{chip}</IssuePreviewHoverCard>
}
