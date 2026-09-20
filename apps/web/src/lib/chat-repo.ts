// EXP-822: the repository a chat run is anchored to.
//
// A chat with no connected repo runs repo-less (EXP-739: a conversation with
// the tracker, in a scratch dir with only the Exponential MCP server wired
// up). With repos connected, EXP-993 anchors every chat to one of them: the
// first by default, another by choice when the team has several. The old
// "No repository" option is gone — the two empty-prompt chat pages once
// shipped without any repository control, every chat ran repo-less and the
// agent resolved its subject by scanning the machine for `.git` directories
// (a stale sibling of the real clone, confidently wrong); offering repo-less
// beside the repos kept that failure one click away.
//
// The rule lives here so the picker's option list, its default and the
// inputs the start carries are one testable thing (EXP-825: the Agent page
// composer is the only chat launcher left).

/** The team's connected repos, as `repositories.list` returns them. */
export interface ChatRepoOption {
  id: string
  fullName: string
}

/** The picker's options — one per connected repo, in the order the server
 * lists them. EXP-993: repo-less is NOT on offer any more, and the composer
 * only renders the picker with two or more (one repo is not a choice). */
export function chatRepoOptions(
  repos: ChatRepoOption[]
): { value: string; label: string }[] {
  return repos.map((repo) => ({ value: repo.id, label: repo.fullName }))
}

/** What the picker starts on: the FIRST connected repo (EXP-993 — a chat is
 * anchored somewhere by default; with several repos the picker lets the
 * person move it). Empty only when the team has no repo at all. */
export function defaultChatRepoId(repos: ChatRepoOption[]): string {
  return repos[0]?.id ?? ``
}

/** The chat builtin's input values for a start. The `repo` key is emitted
 * ONLY when one is picked — an absent key is what the server reads as
 * repo-less, and an empty-string value is not the same thing. EXP-825: the
 * chat text is the start's `prompt` now, not an input, so with no repo there
 * are no inputs at all (`undefined`, the `buildInputsPayload` convention). */
export function chatStartInputs(
  repoId: string
): Record<string, string> | undefined {
  return repoId ? { repo: repoId } : undefined
}
