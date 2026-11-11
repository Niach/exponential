// #FEED-212 (merge-train.json `landing`): THE LANDING CONFIRMATION. The
// worker reports a landing (the merged line, the forge's Closed, pr_merged
// and its issue moves) only after this said yes: the landed commit is new
// (a train head equal to the old base sha landed nothing) and the base, read
// after the push, points at or contains it. #2433: an empty compose made the
// fast-forward a no-op from base_sha to base_sha that "succeeded".
// #FEED-222: GitHub's compare read lags its ref write: right after a
// fast-forward it can still say `behind`. The ref itself is read first, and
// a miss is read again a few times before the landing counts as unlanded.
import { landingConfirmed } from "@exp/domain-contract/merge-train"
import type { Forge, ForgeRepo } from "@/lib/ecode/forge"

/** The waits before each re-read of a base that does not hold it yet
 *  (tests empty it). */
export const confirmRetry = { delaysMs: [2_000, 5_000, 10_000] as readonly number[] }

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

async function holds(forge: Forge, repo: ForgeRepo, base: string, landed: string): Promise<boolean | null> {
  const tip = await forge.readBranch(repo, base).catch(() => null)
  if (tip === landed) return true
  return forge.branchContains(repo, base, landed).catch(() => null)
}

export async function confirmLanding(
  forge: Forge,
  repo: ForgeRepo,
  args: { base: string; from: string | null; landed: string | null },
  delaysMs: readonly number[] = confirmRetry.delaysMs
): Promise<boolean> {
  const { base, from, landed } = args
  if (landed === null || landed === from) return landingConfirmed({ from, landed, contains: null })
  let contains = await holds(forge, repo, base, landed)
  for (const delay of delaysMs) {
    if (contains === true) break
    await sleep(delay)
    contains = await holds(forge, repo, base, landed)
  }
  return landingConfirmed({ from, landed, contains })
}
