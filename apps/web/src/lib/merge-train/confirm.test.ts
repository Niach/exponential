import { describe, expect, it } from "vitest"
import type { Forge, ForgeRepo } from "@/lib/ecode/forge"
import { confirmLanding } from "@/lib/merge-train/confirm"

// #FEED-212: a landing is reported only once the base holds the commit.
const repo: ForgeRepo = { id: `r1`, fullName: `acme/app` }

function forgeWith(
  branchContains: Forge[`branchContains`],
  readBranch: Forge[`readBranch`] = async () => null
): { forge: Forge; asked: string[] } {
  const asked: string[] = []
  const forge = {
    readBranch,
    branchContains: async (r: ForgeRepo, branch: string, sha: string) => {
      asked.push(`${branch}@${sha}`)
      return branchContains(r, branch, sha)
    },
  } as unknown as Forge
  return { forge, asked }
}

const confirm = (forge: Forge, args: { base: string; from: string | null; landed: string | null }) =>
  confirmLanding(forge, repo, args, [0, 0, 0])

describe(`confirmLanding`, () => {
  it(`confirms a base that now points at the landed commit`, async () => {
    const { forge, asked } = forgeWith(async () => true)
    expect(await confirm(forge, { base: `main`, from: `b0`, landed: `c1` })).toBe(true)
    expect(asked).toEqual([`main@c1`])
  })

  it(`race: a later landing moved the base on, still containing it = landed`, async () => {
    const tip = new Map([[`main`, `c2`]])
    const ancestry = new Map([[`c2`, [`c2`, `c1`, `b0`]]])
    const { forge } = forgeWith(async (_r, branch, sha) => ancestry.get(tip.get(branch)!)!.includes(sha))
    expect(await confirm(forge, { base: `main`, from: `b0`, landed: `c1` })).toBe(true)
  })

  it(`race: the base moved to a commit without it = not landed`, async () => {
    const { forge } = forgeWith(async () => false)
    expect(await confirm(forge, { base: `main`, from: `b0`, landed: `c1` })).toBe(false)
  })

  it(`#FEED-222: a base whose ref already points at the landed commit needs no compare`, async () => {
    const { forge, asked } = forgeWith(async () => false, async () => `c1`)
    expect(await confirm(forge, { base: `main`, from: `b0`, landed: `c1` })).toBe(true)
    expect(asked).toEqual([])
  })

  it(`#FEED-222: a stale read right after the push is read again`, async () => {
    let reads = 0
    const { forge, asked } = forgeWith(async () => ++reads > 2)
    expect(await confirm(forge, { base: `main`, from: `b0`, landed: `c1` })).toBe(true)
    expect(asked).toEqual([`main@c1`, `main@c1`, `main@c1`])
  })

  it(`#2433: an empty compose (head = base_sha) never lands, without asking the forge`, async () => {
    const { forge, asked } = forgeWith(async () => true)
    expect(await confirm(forge, { base: `main`, from: `b0`, landed: `b0` })).toBe(false)
    expect(asked).toEqual([])
  })

  it(`a failed push: no commit, an unreadable or a throwing forge = not landed`, async () => {
    expect(await confirm(forgeWith(async () => true).forge, { base: `main`, from: `b0`, landed: null })).toBe(false)
    expect(await confirm(forgeWith(async () => null).forge, { base: `main`, from: `b0`, landed: `c1` })).toBe(false)
    const throwing = forgeWith(async () => {
      throw new Error(`GitHub 502`)
    }).forge
    expect(await confirm(throwing, { base: `main`, from: `b0`, landed: `c1` })).toBe(false)
  })
})
