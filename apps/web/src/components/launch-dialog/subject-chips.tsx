import type { Issue } from "@/db/schema"
import type { TeamAction } from "@/components/action-editor-dialog"
import { Pill } from "@/components/ui/pill"
import { conceptIcon } from "@/lib/icons.generated"
import { getActionIcon } from "@/lib/board-icons"

// EXP-825: the composer's SUBJECT, as chips in the card's leading row — one
// per picked issue, or ONE action chip. They are how "exclusivity by swap"
// reads: picking the other kind replaces what is here, and the chips show it.
// Each chip's trailing glyph removes it (the last issue chip going = a chat
// again). Test ids are byte-identical with the native suites
// (`agent-composer-chip-issue-<IDENT>`, `agent-composer-chip-action`).

const UiCloseIcon = conceptIcon(`ui-close`)
const IssueRefIcon = conceptIcon(`editor-issue-ref`)

export function SubjectChips({
  issues,
  pendingIssueCount,
  action,
  actionPending,
  onRemoveIssue,
  onClearAction,
  disabled,
}: {
  /** The checked issues that have synced rows. */
  issues: Issue[]
  /** Checked ids whose rows have not synced yet (a fresh deep link). */
  pendingIssueCount: number
  /** The subject action's row, when the subject is an action. */
  action: TeamAction | null
  /** The subject is an action whose row is not synced (or gone). */
  actionPending: boolean
  onRemoveIssue: (issueId: string) => void
  onClearAction: () => void
  disabled?: boolean
}) {
  if (action || actionPending) {
    const RowIcon = action ? getActionIcon(action) : IssueRefIcon
    return (
      <Pill
        size="md"
        mode="action"
        selected
        data-testid="agent-composer-chip-action"
        aria-label={`Remove ${action?.name ?? `action`}`}
        title="Remove"
        disabled={disabled}
        onClick={onClearAction}
        leading={<RowIcon className="size-3.5" />}
      >
        <span className="max-w-[16rem] truncate">{action?.name ?? `Action…`}</span>
        <UiCloseIcon className="size-3 text-muted-foreground" />
      </Pill>
    )
  }
  return (
    <>
      {issues.map((issue) => (
        <Pill
          key={issue.id}
          size="md"
          mode="action"
          selected
          data-testid={`agent-composer-chip-issue-${issue.identifier}`}
          aria-label={`Remove ${issue.identifier}`}
          title={issue.title}
          disabled={disabled}
          onClick={() => onRemoveIssue(issue.id)}
        >
          <span className="font-mono text-xs">{issue.identifier}</span>
          <span className="max-w-[14rem] truncate">{issue.title}</span>
          <UiCloseIcon className="size-3 text-muted-foreground" />
        </Pill>
      ))}
      {pendingIssueCount > 0 && (
        <Pill size="md">{`${pendingIssueCount} loading…`}</Pill>
      )}
    </>
  )
}
