import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import { IssuePickerHost } from "@/components/issue-picker-host"

interface IssuePickerDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onPick: (issue: ResolvedIssueRef) => void
  /** Issues to hide from results (e.g. the issue being marked). */
  excludeIssueIds?: string[]
  title?: string
  placeholder?: string
}

const NO_ROWS: ResolvedIssueRef[] = []

// The relation linker and the mark-as-duplicate picker: THE issue picker host
// (`issue-picker-host.tsx`) in its `dialog` shell — both callers open it from
// a MENU ROW, so there is no trigger for a popover to hang off, and a phone
// gets the full sheet. Kept as a named binding over the team's issue refs.
export function IssuePickerDialog({
  open,
  onOpenChange,
  onPick,
  excludeIssueIds,
  title = `Select issue`,
  placeholder = `Search issues…`,
}: IssuePickerDialogProps) {
  const issueRefs = useIssueRefs()
  return (
    <IssuePickerHost
      shell="dialog"
      mode="single"
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      teamId={issueRefs?.teamId}
      rows={open ? (issueRefs?.rows ?? NO_ROWS) : NO_ROWS}
      resolveHit={(hit) => issueRefs?.fromHit(hit) ?? null}
      limit={30}
      exclude={excludeIssueIds}
      searchPlaceholder={placeholder}
      onPick={onPick}
      // EXP-962: the in-list empty line, no icon column.
      emptyText={(query) =>
        query.trim() ? `No issues match "${query}"` : `No issues to pick from`
      }
      className="min-h-0 bg-transparent **:data-[slot=command-input-wrapper]:border-border/50"
      listClassName="max-h-none flex-1 overflow-y-auto"
    />
  )
}
