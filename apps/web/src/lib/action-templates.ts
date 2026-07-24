// Starter templates for team actions (EXP-253). An action body is a GFM
// markdown prompt executed by an interactive claude session on a member's
// desktop with the exponential MCP tools available — templates prefill the
// editor and stay fully editable before save.

export interface ActionTemplate {
  id: string
  name: string
  description: string
  /** Preselect a repository in the editor (the run clones it). */
  wantsRepo: boolean
  body: string
}

export const ACTION_TEMPLATES: ActionTemplate[] = [
  {
    id: `code-review`,
    name: `Code review → file issues`,
    description: `Review recent commits on the default branch and file issues for real defects.`,
    wantsRepo: true,
    body: `Review the recent work on this repository's default branch and file issues for anything genuinely wrong.

1. Use \`exponential_teams_list\` and \`exponential_boards_list\` to find the board that backs this repository — new issues go there.
2. Scan the recent commits with \`git log --oneline -30\` and read the interesting ones with \`git show\`. Focus on the last week or so of changes.
3. Look for **real defects only**: bugs, race conditions, missing error handling, security problems, broken edge cases. Do not file style nits, refactor wishes, or hypotheticals.
4. Before filing anything, check \`exponential_issues_list\` for the board so you never file a duplicate of an existing issue.
5. File each defect with \`exponential_issues_create\`: a short title, a description with the file/line, why it is wrong, and a suggested fix, plus a sensible priority (urgent only for data loss or breakage).

Finish with a summary of what you filed and what you looked at but left alone.`,
  },
  {
    id: `triage`,
    name: `Label + prioritize all issues`,
    description: `Sweep open issues, apply fitting labels, and set sensible priorities.`,
    wantsRepo: false,
    body: `Triage the open issues in this team: label and prioritize them consistently.

1. Learn the landscape first: \`exponential_boards_list\` for the boards, \`exponential_labels_list\` for the existing label vocabulary of each board.
2. Page through the open issues with \`exponential_issues_list\` — skip anything done, cancelled, or duplicate.
3. Add fitting **existing** labels with \`exponential_issue_labels_add\`. Only create a new label with \`exponential_labels_create\` when several issues clearly need it and nothing existing fits.
4. Set priority with \`exponential_issues_update\`:
   - **urgent** — breakage or data loss
   - **high** — user-visible bugs
   - **medium** — solid improvements
   - **low** — nice-to-haves
5. Leave issues that are already labeled and prioritized correctly alone.

End with a summary of what changed and anything you were unsure about.`,
  },
  {
    id: `changelog`,
    name: `Draft changelog from recent merges`,
    description: `Turn recently completed work into a short user-facing changelog draft.`,
    wantsRepo: false,
    body: `Draft a user-facing changelog entry from this team's recently completed work.

1. Collect the completed work with \`exponential_issues_list\`: status done, completed within roughly the last 2 weeks, across the team's boards.
2. Group the items into user-visible themes. Drop internal refactors, chores, and anything a user would never notice.
3. Write a short GFM entry: a one-sentence summary line, then 3–6 bullets, each starting with a **bold** feature phrase followed by a plain-language sentence. No issue identifiers, no jargon.

Post the draft as your final message. This action drafts only — do not create, edit, or close anything.`,
  },
]
