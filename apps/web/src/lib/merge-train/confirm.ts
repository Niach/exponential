// #FEED-212 (merge-train.json `landing`): THE LANDING CONFIRMATION. The
// worker reports a landing (the merged line, the forge's Closed, pr_merged
// and its issue moves) only after this said yes: the landed commit is new
// (a train head equal to the old base sha landed nothing) and the base, read
// after the push, points at or contains it. #2433: an empty compose made the
// fast-forward a no-op from base_sha to base_sha that "succeeded".
import { landingConfirmed } from "@exp/domain-contract/merge-train"
import type { Forge, ForgeRepo } from "@/lib/ecode/forge"

export async function confirmLanding(
  forge: Forge,
  repo: ForgeRepo,
  args: { base: string; from: string | null; landed: string | null }
): Promise<boolean> {
  const { base, from, landed } = args
  if (landed === null || landed === from) return landingConfirmed({ from, landed, contains: null })
  const contains = await forge.branchContains(repo, base, landed).catch(() => null)
  return landingConfirmed({ from, landed, contains })
}
