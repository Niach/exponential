import { describe, expect, it } from "vitest"
import { nodeGuardVerdict, pinnedNodeMajor } from "./node-guard"

describe(`pinnedNodeMajor`, () => {
  it(`reads the nodejs major out of a .tool-versions file`, () => {
    expect(pinnedNodeMajor(`nodejs 24.11.1\ncaddy 2.10.2\n`)).toBe(24)
  })

  it(`ignores other tools and other lines`, () => {
    expect(pinnedNodeMajor(`caddy 2.10.2\nruby 3.3.0\n`)).toBeNull()
    // Not anchored mid-line: `caddy nodejs 26` must not read as a pin.
    expect(pinnedNodeMajor(`caddy nodejs 26\n`)).toBeNull()
  })

  it(`returns null for an empty or malformed file`, () => {
    expect(pinnedNodeMajor(``)).toBeNull()
    expect(pinnedNodeMajor(`nodejs lts\n`)).toBeNull()
  })
})

describe(`nodeGuardVerdict`, () => {
  it(`passes on the pinned major`, () => {
    expect(nodeGuardVerdict({ current: `24.11.1`, pinned: 24, isBun: false })).toEqual({
      kind: `ok`,
    })
  })

  it(`warns on an unpinned but working major`, () => {
    const verdict = nodeGuardVerdict({
      current: `22.12.0`,
      pinned: 24,
      isBun: false,
    })
    expect(verdict.kind).toBe(`warn`)
    expect(verdict.message).toContain(`is not Node 24`)
  })

  // The whole point of the guard: Node 26 serves a dev app that renders and
  // never syncs, with no error anywhere.
  it(`fails on Node 26 and up`, () => {
    const verdict = nodeGuardVerdict({
      current: `26.0.0`,
      pinned: 24,
      isBun: false,
    })
    expect(verdict.kind).toBe(`fail`)
    expect(verdict.message).toContain(`cannot serve this app in dev`)
    expect(verdict.message).toContain(`node@24`)
  })

  it(`still fails on Node 26 without a .tool-versions pin`, () => {
    const verdict = nodeGuardVerdict({
      current: `27.1.0`,
      pinned: null,
      isBun: false,
    })
    expect(verdict.kind).toBe(`fail`)
    expect(verdict.message).toContain(`the version in .tool-versions`)
  })

  it(`warns, not fails, when .tool-versions is missing on a working major`, () => {
    expect(
      nodeGuardVerdict({ current: `24.11.1`, pinned: null, isBun: false }).kind
    ).toBe(`warn`)
  })

  // Bun reports an emulated `process.versions.node` that says nothing about
  // the runtime actually serving — and Bun's fetch has no unix-socket bug.
  it(`passes under Bun regardless of the reported Node version`, () => {
    const verdict = nodeGuardVerdict({
      current: `26.0.0`,
      pinned: 24,
      isBun: true,
    })
    expect(verdict.kind).toBe(`ok`)
    expect(verdict.message).toContain(`Bun`)
  })
})
