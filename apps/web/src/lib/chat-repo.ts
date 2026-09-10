// EXP-822: the repository a chat run is anchored to.
//
// A chat may legitimately have no repository (EXP-739: a conversation with
// the tracker, run in a scratch dir with only the Exponential MCP server
// wired up) — but "no repository" has to be a CHOICE the person made, not
// the only thing the surface can express. The two empty-prompt chat pages
// (this app's `t/$teamSlug/agent` and the desktop's `Screen::Chat`) shipped
// without any repository control at all, so every chat started from them ran
// repo-less and the agent resolved its subject by scanning the machine for
// `.git` directories. It found a stale sibling of the real clone and got
// everything after that confidently wrong.
//
// The rule lives here so the picker's option list, its default and the
// inputs the start carries are one testable thing, shared with the launch
// dialog's `ChatPane` sentinel semantics.

/** The team's connected repos, as `repositories.list` returns them. */
export interface ChatRepoOption {
  id: string
  fullName: string
}

/** Radix/the dropdown forbid an empty item value, so repo-less rides a
 * sentinel inside the picker only; `` is what the server reads (an absent
 * `repo` input, `lib/trpc/steer.ts`). Same idea as `ChatPane`'s own. */
export const NO_REPO = `no-repo`

export const NO_REPO_LABEL = `No repository`

/** The picker's options, repo-less first. EMPTY when the team has no repo
 * connected: there is nothing to choose between, so the chat pages render no
 * picker rather than a one-entry menu reading "No repository". */
export function chatRepoOptions(
  repos: ChatRepoOption[]
): { value: string; label: string }[] {
  if (repos.length === 0) return []
  return [
    { value: NO_REPO, label: NO_REPO_LABEL },
    ...repos.map((repo) => ({ value: repo.id, label: repo.fullName })),
  ]
}

/** What the picker starts on. Exactly one repo pre-picks it (the rule the
 * launch dialog has used since EXP-615: there is nothing to pick); with
 * several, repo-less stays the default and the run's own prompt makes the
 * agent ASK which one rather than guess. */
export function defaultChatRepoId(repos: ChatRepoOption[]): string {
  return repos.length === 1 ? repos[0]!.id : ``
}

/** The chat builtin's input values for a start. The `repo` key is emitted
 * ONLY when one is picked — an absent key is what the server reads as
 * repo-less, and an empty-string value is not the same thing. */
export function chatStartInputs(
  prompt: string,
  repoId: string
): Record<string, string> {
  return { prompt, ...(repoId ? { repo: repoId } : {}) }
}
