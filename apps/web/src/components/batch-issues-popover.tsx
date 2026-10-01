import { useMemo, useState } from "react"
import { Link } from "@tanstack/react-router"
import {
  Button,
  IssueChipStack,
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@exp/ui"
import type { Issue } from "@/db/schema"
import { IssueChip } from "@/components/issue-chip"
import { useTeamBoards } from "@/hooks/use-team-data"

// A multi-issue run's subject in the work header: its covered issues as ONE
// stacked chip (`EXP-874 +2`, `IssueChipStack`) that opens a popover listing
// every one of them, each opening its issue. A single-issue run has nothing
// here — its issue is the header itself.

export function BatchIssuesPopover({
  teamId,
  teamSlug,
  issues,
}: {
  teamId: string
  teamSlug: string
  /** The covered issues in naming order (`batchRunIssues`). */
  issues: readonly Issue[]
}) {
  const [open, setOpen] = useState(false)
  const boards = useTeamBoards(teamId)
  const boardSlugById = useMemo(
    () => new Map(boards.map((board) => [board.id, board.slug])),
    [boards]
  )
  const first = issues[0]
  if (!first || issues.length < 2) return null
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 min-w-0 shrink px-1.5"
          aria-label={`${issues.length} issues`}
          data-testid="batch-issues-trigger"
        >
          <IssueChipStack count={issues.length - 1}>
            <IssueChip issue={first} size="sm" preview={false} />
          </IssueChipStack>
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="flex w-72 flex-col gap-1.5 p-2"
        data-testid="batch-issues-popover"
      >
        {issues.map((issue) => {
          const boardSlug = boardSlugById.get(issue.boardId)
          return (
            <IssueChip
              key={issue.id}
              issue={issue}
              className="w-full"
              testId={`batch-issue-${issue.identifier}`}
              link={
                boardSlug
                  ? (props) => (
                      <Link
                        to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                        params={{
                          teamSlug,
                          boardSlug,
                          issueIdentifier: issue.identifier,
                        }}
                        onClick={() => setOpen(false)}
                        {...props}
                      />
                    )
                  : undefined
              }
            />
          )
        })}
      </PopoverContent>
    </Popover>
  )
}
