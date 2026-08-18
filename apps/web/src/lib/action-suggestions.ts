// Curated action-suggestion seeds (EXP-530) — the Suggestions tab renders
// them as cards and "Use suggestion" opens the create-action dialog with the
// description prefilled. Each description is written as INSTRUCTIONS for what
// the authored action's prompt should do: it becomes the builtin "Create
// action" run's description input, so the creator agent acts on it verbatim.
// Icons come from the curated boardIcon registry set (never a raw glyph name
// outside it — the swatch grid and natives only know that set).

export interface ActionSuggestion {
  id: string
  title: string
  description: string
  icon: string
}

export const ACTION_SUGGESTIONS: ActionSuggestion[] = [
  {
    id: `daily-standup-digest`,
    title: `Daily standup digest`,
    description: `Summarize what changed across the team's boards in the last 24 hours: issues created, completed, and moved, plus open pull requests. Post the digest as a comment on a dedicated standup issue, grouped by board with issue identifiers linked.`,
    icon: `calendar`,
  },
  {
    id: `backlog-grooming-sweep`,
    title: `Backlog grooming sweep`,
    description: `Review backlog issues and flag ones that are missing a description, a priority, or labels. Leave a short comment on each flagged issue listing what is missing, and skip issues already flagged in a previous run.`,
    icon: `boxes`,
  },
  {
    id: `stale-issue-nudge`,
    title: `Stale-issue nudge`,
    description: `Find in-progress issues with no updates for 14 days or more. Comment on each one asking the assignee for a status update, mentioning them by email, and include how long the issue has been quiet.`,
    icon: `clock`,
  },
  {
    id: `release-notes-drafter`,
    title: `Release notes drafter`,
    description: `Collect issues completed since the last run and draft user-facing release notes from their titles and descriptions. Group entries into Features, Fixes, and Improvements, and file the draft as a new issue for review.`,
    icon: `file-text`,
  },
  {
    id: `pr-review-summarizer`,
    title: `PR review summarizer`,
    description: `For each open pull request linked to a team issue, read the changed files and post a concise review summary as a comment on the linked issue: what the change does, notable risks, and suggested test focus areas.`,
    icon: `git-branch`,
  },
  {
    id: `widget-bug-triage`,
    title: `Bug triage on new widget feedback`,
    description: `Triage newly created widget-reported issues: rewrite vague titles to be specific, set a priority based on severity, add reproduction steps when they can be inferred from the report, and label likely duplicates.`,
    icon: `bug`,
  },
  {
    id: `weekly-metrics-comment`,
    title: `Weekly metrics comment`,
    description: `Compute weekly team metrics: issues created versus completed, average time to done, and open pull request count. Post the numbers with a week-over-week comparison as a comment on a dedicated metrics issue.`,
    icon: `chart-line`,
  },
  {
    id: `label-janitor`,
    title: `Label janitor`,
    description: `Keep labels tidy: find unlabeled issues and apply the best-fitting existing labels based on title and description. Never create new labels, and list every change made in a summary comment on a dedicated janitor issue.`,
    icon: `flag`,
  },
]
