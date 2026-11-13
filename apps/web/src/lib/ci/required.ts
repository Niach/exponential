// #EXP-1617: a repository's own required checks (`repositories.ci_config`)
// gate the merge train ON TOP of the forge's protection: the union, in the
// forge's order then the repo's.

import type { RepositoryCiConfig } from "@exp/db-schema"

export function withRepoRequired(config: RepositoryCiConfig | null | undefined, forgeRequired: readonly string[]): string[] {
  const own = (config?.requiredChecks ?? []).filter((name) => typeof name === `string` && name.length > 0)
  return [...new Set([...forgeRequired, ...own])]
}
