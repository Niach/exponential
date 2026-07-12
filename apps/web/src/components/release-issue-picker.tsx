import { Rocket } from "lucide-react"
import type { Issue } from "@/db/schema"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"

// Shared multi-select issue picker for release surfaces (EXP-62): the
// creation dialog (pick issues BEFORE the release exists) and the detail's
// "Add issues" dialog render the same Command search + checkbox rows.
// Issues bundled into ANOTHER release stay offered (adding MOVES them — the
// server records both timeline sides) with a badge showing where they
// currently live.

// The shared candidate rule: closed statuses and archived issues are
// excluded — they have nothing left to ship. `excludeReleaseId` drops issues
// already in the target release (the add dialog); the creation dialog has no
// release yet, so it passes nothing.
export function releaseCandidateIssues(
  issues: Issue[],
  excludeReleaseId?: string
): Issue[] {
  return issues
    .filter(
      (issue) =>
        (excludeReleaseId === undefined ||
          issue.releaseId !== excludeReleaseId) &&
        issue.archivedAt === null &&
        issue.status !== `done` &&
        issue.status !== `cancelled` &&
        issue.status !== `duplicate`
    )
    .sort((a, b) => a.identifier.localeCompare(b.identifier))
}

export function ReleaseIssuePicker({
  candidates,
  selectedIds,
  onToggle,
  releaseNameById,
}: {
  candidates: Issue[]
  selectedIds: Set<string>
  onToggle: (issueId: string) => void
  releaseNameById: Map<string, string>
}) {
  return (
    <Command className="flex-1 overflow-hidden">
      <CommandInput placeholder="Search issues..." />
      <CommandList className="max-h-none flex-1">
        <CommandEmpty>No issues found.</CommandEmpty>
        <CommandGroup>
          {candidates.map((issue) => {
            const otherReleaseName = issue.releaseId
              ? releaseNameById.get(issue.releaseId)
              : undefined
            return (
              <CommandItem
                key={issue.id}
                value={`${issue.identifier} ${issue.title}`}
                onSelect={() => onToggle(issue.id)}
                className="flex items-center gap-2"
              >
                <Checkbox
                  checked={selectedIds.has(issue.id)}
                  className="pointer-events-none"
                />
                <StatusIcon
                  status={issue.status}
                  className="size-3.5 shrink-0"
                />
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {issue.identifier}
                </span>
                <span className="min-w-0 truncate text-sm">{issue.title}</span>
                {issue.releaseId && (
                  <Badge
                    variant="outline"
                    className="ml-auto shrink-0 text-muted-foreground"
                  >
                    <Rocket className="size-2.5" />
                    {otherReleaseName ?? `Another release`}
                  </Badge>
                )}
              </CommandItem>
            )
          })}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}
