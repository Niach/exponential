import { describe, expect, it } from "vitest"
import type { Forge, ForgeRepo } from "@/lib/ecode/forge"
import { confirmLanding } from "@/lib/merge-train/confirm"

// #FEED-212: a landing is reported only once the base holds the commit.
const repo: ForgeRepo = { id: `r1`, fullName: `acme/app` }

function forgeWith(branchContains: Forge[`branchContains`]): { forge: Forge; asked: string[] } {
  const asked: string[] = []
  const forge = {
    branchContains: async (r: ForgeRepo, branch: string, sha: string) => {
      asked.push(`${branch}@${sha}`)
      return branchContains(r, branch, sha)
    },
  } as unknown as Forge
  return { forge, asked }
}

describe(`confirmLanding`, () => {
  it(`confirms a base that now points at the landed commit`, async () => {
    const { forge, asked } = forgeWith(async () => true)
    expect(await confirmLanding(forge, repo, { base: `main`, from: `b0`, landed: `c1` })).toBe(true)
    expect(asked).toEqual([`main@c1`])
  })

  it(`race: a later landing moved the base on, still containing it = landed`, async () => {
    const tip = new Map([[`main`, `c2`]])
    const ancestry = new Map([[`c2`, [`c2`, `c1`, `b0`]]])
    const { forge } = forgeWith(async (_r, branch, sha) => ancestry.get(tip.get(branch)!)!.includes(sha))
    expect(await confirmLanding(forge, repo, { base: `main`, from: `b0`, landed: `c1` })).toBe(true)
  })

  it(`race: the base moved to a commit without it = not landed`, async () => {
    const { forge } = forgeWith(async () => false)
    expect(await confirmLanding(forge, repo, { base: `main`, from: `b0`, landed: `c1` })).toBe(false)
  })

  it(`#2433: an empty compose (head = base_sha) never lands, without asking the forge`, async () => {
    const { forge, asked } = forgeWith(async () => true)
    expect(await confirmLanding(forge, repo, { base: `main`, from: `b0`, landed: `b0` })).toBe(false)
    expect(asked).toEqual([])
  })

  it(`a failed push: no commit, an unreadable or a throwing forge = not landed`, async () => {
    expect(await confirmLanding(forgeWith(async () => true).forge, repo, { base: `main`, from: `b0`, landed: null })).toBe(false)
    expect(await confirmLanding(forgeWith(async () => null).forge, repo, { base: `main`, from: `b0`, landed: `c1` })).toBe(false)
    const throwing = forgeWith(async () => {
      throw new Error(`GitHub 502`)
    }).forge
    expect(await confirmLanding(throwing, repo, { base: `main`, from: `b0`, landed: `c1` })).toBe(false)
  })
})
