// Options for a `pr`-typed action input (EXP-259): the team's OPEN pull
// requests. EXP-1293: PR state left the issues; a PR is a synced
// `pull_requests` row (its closed issues ride `pull_request_issues`), so a
// row is one option whose VALUE is the pull request's id. The label is the
// PR number, then the issues it closes (else its title).

export interface PrOptionPull {
  id: string
  teamId: string
  number: number
  title: string | null
  state: string
}

export interface PrOptionLink {
  pullRequestId: string
  issueId: string
}

export interface PrOption {
  /** The pull request's id: the value the `pr` input submits. */
  pullRequestId: string
  number: number
  /** Every issue identifier the pull request closes, sorted. */
  identifiers: string[]
  /** Every linked issue's id: the set `findPrOption` matches on. */
  linkedIssueIds: string[]
  /** `#42 · EXP-1, EXP-2`, else `#42 · <title>`, else `#42`. */
  label: string
}

/**
 * One option per OPEN pull request of `teamId`, sorted by number (newest
 * first) so the list never reshuffles as sync lands rows. `identifierById`
 * resolves the linked issues (an unsynced one is skipped in the label but
 * still matched by id).
 */
export function buildPrOptions(
  pulls: readonly PrOptionPull[],
  links: readonly PrOptionLink[],
  identifierById: ReadonlyMap<string, string>,
  teamId: string
): PrOption[] {
  const linksByPull = new Map<string, string[]>()
  for (const link of links) {
    const list = linksByPull.get(link.pullRequestId) ?? []
    list.push(link.issueId)
    linksByPull.set(link.pullRequestId, list)
  }
  return pulls
    .filter((pull) => pull.teamId === teamId && pull.state === `open`)
    .map((pull) => {
      const linkedIssueIds = [...new Set(linksByPull.get(pull.id) ?? [])].sort()
      const identifiers = linkedIssueIds
        .map((id) => identifierById.get(id))
        .filter((ident): ident is string => Boolean(ident))
        .sort()
      const tail = identifiers.length > 0 ? identifiers.join(`, `) : pull.title?.trim()
      return {
        pullRequestId: pull.id,
        number: pull.number,
        identifiers,
        linkedIssueIds,
        label: tail ? `#${pull.number} · ${tail}` : `#${pull.number}`,
      }
    })
    .sort(
      (left, right) =>
        right.number - left.number ||
        left.pullRequestId.localeCompare(right.pullRequestId)
    )
}

/**
 * The option a seed names: the pull request's own id, or ANY issue it closes
 * (a play button on an issue holds the issue, not the PR).
 */
export function findPrOption(
  options: readonly PrOption[],
  id: string
): PrOption | null {
  return (
    options.find((option) => option.pullRequestId === id) ??
    options.find((option) => option.linkedIssueIds.includes(id)) ??
    null
  )
}
