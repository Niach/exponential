// #EXP-2172: what packages/domain-contract/fixtures/ci-page-view.json pins,
// computed by the web reference from each case's inputs. Its test replays
// the fixture; `scripts/bless-ci-page-view.ts` rewrites the trees after a
// deliberate change.
import pageFixture from "@exp/domain-contract/fixtures/ci-page.json"
import type { CiPageInput, CiRunFilter, CiTab } from "@exp/domain-contract/ci-page"
import { ciPageTree, ciRepoRunsTree } from "@/lib/ci-page-view"

type Json = Record<string, unknown>

/** A case's world: ci-page.json's `scenario`, or `empty` (no rows at all). */
export function ciPageWorld(world: string): CiPageInput {
  const scenario = pageFixture.scenario as CiPageInput
  if (world === `empty`) return { ...scenario, runs: [], jobs: [], pullRequests: [], runners: [] }
  return scenario
}

export function replayCiPage(c: Json): unknown {
  // A `repositoryId` case is the repo page's Runs tab (#EXP-2169).
  if (typeof c.repositoryId === `string`) return ciRepoRunsTree(ciPageWorld(c.world as string), c.repositoryId, c.phone === true)
  return ciPageTree(ciPageWorld(c.world as string), {
    tab: c.tab as CiTab,
    filter: c.filter as CiRunFilter,
    phone: c.phone === true,
  })
}
