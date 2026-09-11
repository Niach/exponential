// The ONE "owner/name" shape every repo-by-name entry point accepts — the
// tRPC schemas (repositories.add, boards.create inline, integrations.github.
// lookupRepo) and the picker's by-name field (FEED-30) validate against the
// same pattern so a name the client lets through is never a name the server
// rejects on shape alone.
export const REPO_FULL_NAME_RE = /^[^/\s]+\/[^/\s]+$/

export function isRepoFullName(value: string): boolean {
  return REPO_FULL_NAME_RE.test(value)
}
