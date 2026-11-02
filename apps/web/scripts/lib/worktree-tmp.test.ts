import { describe, expect, it } from "vitest"
import { worktreeKey, worktreeTmpPath } from "./worktree-tmp"

describe(`worktreeKey`, () => {
  it(`is 12 hex, stable for one root and distinct across roots`, () => {
    const a = worktreeKey(`/repos/exponential.worktrees/a`)
    expect(a).toMatch(/^[0-9a-f]{12}$/)
    expect(worktreeKey(`/repos/exponential.worktrees/a/`)).toBe(a)
    expect(worktreeKey(`/repos/exponential.worktrees/b`)).not.toBe(a)
  })
})

describe(`worktreeTmpPath`, () => {
  it(`puts the key between the stem and the extension`, () => {
    const key = worktreeKey(`/w`)
    expect(worktreeTmpPath(`/w`, `exp-screenshot-enet-feed`, `.json`, `/tmp`)).toBe(
      `/tmp/exp-screenshot-enet-feed-${key}.json`
    )
    expect(worktreeTmpPath(`/w`, `exp-screenshot-enet-identity`, ``, `/tmp`)).toBe(
      `/tmp/exp-screenshot-enet-identity-${key}`
    )
  })
})
