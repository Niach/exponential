import { useState } from "react"
import {
  Bot,
  GitBranch,
  GitMerge,
  GitPullRequest,
  Github,
  Loader2,
} from "lucide-react"
import type { Issue, Project, User } from "@/db/schema"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DiffView } from "@/components/diff-view"

function statusText(issue: Issue): string {
  if (issue.prState === `merged`) return `PR merged`
  if (issue.prState === `open`) return `PR open`
  switch (issue.agentPlanState) {
    case `drafting`:
    case `planning`:
      return `Planning…`
    case `awaiting_approval`:
      return `Plan ready for review`
    case `awaiting_answer`:
      return `Waiting for your answer`
    case `approved`:
    case `coding`:
      return `Coding…`
    case `in_review`:
      return `In review`
    case `pushed`:
      return `Pushed`
    default:
      return `Assigned`
  }
}

const working = (issue: Issue) =>
  issue.agentPlanState === `drafting` ||
  issue.agentPlanState === `planning` ||
  issue.agentPlanState === `coding` ||
  issue.agentPlanState === `approved`

// Linear-style agent panel (screenshot 1): the agent actor, its current status,
// the repo/branch it's working in, and PR linkage + a diff view. Shown on the
// issue detail when the assignee is an agent.
export function AgentPanel({
  issue,
  project,
  agent,
}: {
  issue: Issue
  project: Project
  agent: User
}) {
  const [showDiff, setShowDiff] = useState(false)

  return (
    <div className="mx-4 my-3 rounded-md border border-border bg-muted/20">
      <div className="flex items-center gap-2 px-3 py-2">
        <Bot className="size-4 text-indigo-300" />
        <span className="text-sm font-medium">{agent.name}</span>
        <Badge variant="secondary" className="gap-1">
          {working(issue) && <Loader2 className="size-3 animate-spin" />}
          {statusText(issue)}
        </Badge>
        {issue.prMergedAt && (
          <Badge variant="outline" className="gap-1 text-emerald-400">
            <GitMerge className="size-3" /> Merged
          </Badge>
        )}
      </div>

      <div className="space-y-1 px-3 pb-2 text-xs text-muted-foreground">
        {project.githubRepo && (
          <div className="flex items-center gap-1.5">
            <Github className="size-3" />
            <span className="font-mono">{project.githubRepo}</span>
          </div>
        )}
        {issue.branch && (
          <div className="flex items-center gap-1.5">
            <GitBranch className="size-3" />
            <span className="font-mono">{issue.branch}</span>
          </div>
        )}
      </div>

      {issue.prUrl && (
        <div className="flex items-center gap-2 border-t border-border px-3 py-2">
          <a
            href={issue.prUrl}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1.5 text-xs text-indigo-300 hover:underline"
          >
            <GitPullRequest className="size-3.5" />
            {issue.prNumber ? `PR #${issue.prNumber}` : `View pull request`}
          </a>
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto h-6 px-2 text-xs"
            onClick={() => setShowDiff((v) => !v)}
          >
            {showDiff ? `Hide changes` : `View changes`}
          </Button>
        </div>
      )}

      {showDiff && issue.prUrl && (
        <div className="border-t border-border">
          <DiffView issueId={issue.id} />
        </div>
      )}
    </div>
  )
}
