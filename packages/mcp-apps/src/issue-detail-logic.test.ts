import {
  commentCaption,
  startArgs,
  startTargets,
  threadComments,
} from "./issue-detail-logic"

describe(`startTargets`, () => {
  it(`offers online devices only, default first, one row per agent`, () => {
    const targets = startTargets([
      { deviceId: `a`, label: `Zed box`, online: true, agents: [`claude`] },
      { deviceId: `b`, label: `Laptop`, online: false, agents: [`claude`] },
      { deviceId: `c`, label: `Mac`, online: true, isDefault: true, agents: [`claude`, `codex`] },
      { deviceId: `d`, label: `Bare`, online: true, agents: [] },
    ])
    expect(targets.map((target) => target.key)).toEqual([
      `c:claude`,
      `c:codex`,
      `d`,
      `a:claude`,
    ])
    expect(targets[1].label).toBe(`Codex on Mac`)
    expect(targets[2].agent).toBeNull()
  })

  it(`is empty with no online device`, () => {
    expect(startTargets([{ deviceId: `a`, online: false }])).toEqual([])
  })

  it(`names a shared server's owner`, () => {
    const [target] = startTargets([
      { deviceId: `s`, label: `CI`, online: true, owner: { name: `Ann` } },
    ])
    expect(target.label).toBe(`CI (Ann)`)
  })
})

describe(`startArgs`, () => {
  it(`passes the agent only when the target names one`, () => {
    const [withAgent, bare] = startTargets([
      { deviceId: `a`, label: `A`, online: true, agents: [`codex`] },
      { deviceId: `b`, label: `B`, online: true },
    ])
    expect(startArgs(withAgent, `i`)).toEqual({ deviceId: `a`, issueId: `i`, agent: `codex` })
    expect(startArgs(bare, `i`)).toEqual({ deviceId: `b`, issueId: `i` })
  })
})

describe(`threadComments`, () => {
  it(`orders roots oldest first with their replies under them`, () => {
    const rows = [
      { id: `r2`, parentId: `a`, createdAt: `2026-01-04T00:00:00Z` },
      { id: `b`, parentId: null, createdAt: `2026-01-03T00:00:00Z` },
      { id: `r1`, parentId: `a`, createdAt: `2026-01-02T00:00:00Z` },
      { id: `a`, parentId: null, createdAt: `2026-01-01T00:00:00Z` },
      { id: `orphan`, parentId: `gone`, createdAt: `2026-01-05T00:00:00Z` },
    ]
    expect(threadComments(rows).map(({ comment, reply }) => `${comment.id}${reply ? `*` : ``}`)).toEqual([
      `a`,
      `r1*`,
      `r2*`,
      `b`,
      `orphan`,
    ])
  })
})

describe(`commentCaption`, () => {
  it(`captions MCP and reporter comments only`, () => {
    expect(commentCaption(`mcp`)).toBe(`via MCP`)
    expect(commentCaption(`reporter`)).toBe(`reporter`)
    expect(commentCaption(`user`)).toBeNull()
  })
})
