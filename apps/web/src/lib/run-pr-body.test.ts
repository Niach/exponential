import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1154: the PR body follows the run's report. The db read, the GitHub
// token and the PATCH are mocked; the pure projection runs for real.

const state = vi.hoisted(() => ({
  row: null as Record<string, unknown> | null,
  selectThrows: false,
  token: { token: `tok`, installationId: 1, expiresAt: 0 } as unknown,
  patch: vi.fn(async (_opts: unknown) => {}),
}))

vi.mock(`@/db/connection`, () => {
  const chain: Record<string, unknown> = {}
  for (const name of [`from`, `leftJoin`, `where`]) chain[name] = () => chain
  chain.limit = async () => {
    if (state.selectThrows) throw new Error(`db down`)
    return state.row ? [state.row] : []
  }
  return { db: { select: () => chain } }
})
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoInstallationTokenInfo: async () => state.token,
}))
vi.mock(`@/lib/trpc/pr-update`, () => ({
  patchPullDescription: (opts: unknown) => state.patch(opts),
}))

import {
  loadRunReport,
  pendingRunPrBodySyncs,
  runPrBody,
  syncRunPrBody,
} from "./run-pr-body"

const report = [{ topic: `Summary`, label: null, attachmentId: null, text: `Did it`, files: [`a.ts`] }]
const baseRow = {
  id: `s1`,
  results: report,
  prUrl: `https://github.com/acme/web/pull/7`,
  prNumber: 7,
  prState: `open`,
  issueIdentifier: `EXP-1`,
  boardSlug: `web`,
  teamSlug: `acme`,
}

beforeEach(() => {
  process.env.BETTER_AUTH_URL = `https://exp.example/`
  state.row = { ...baseRow }
  state.selectThrows = false
  state.token = { token: `tok`, installationId: 1, expiresAt: 0 }
  state.patch.mockReset()
})

describe(`loadRunReport`, () => {
  it(`links an issue run to the issue's Guide`, async () => {
    expect((await loadRunReport(`s1`))?.resultsUrl).toBe(
      `https://exp.example/t/acme/boards/web/issues/EXP-1?view=guide`
    )
  })

  it(`links a run without an issue to the run's page`, async () => {
    state.row = { ...baseRow, issueIdentifier: null, boardSlug: null }
    expect((await loadRunReport(`s1`))?.resultsUrl).toBe(
      `https://exp.example/t/acme/sessions/s1?view=guide`
    )
  })
})

describe(`runPrBody`, () => {
  it(`derives the body from the report`, async () => {
    const out = await runPrBody(`s1`, `agent body`)
    expect(out.fromResults).toBe(true)
    expect(out.body).toContain(`Did it\n\n- \`a.ts\``)
    expect(out.body).toContain(`(https://exp.example/t/acme/boards/web/issues/EXP-1?view=guide)`)
  })

  it(`keeps the fallback without a run, without report text or on a failed read`, async () => {
    expect(await runPrBody(null, `x`)).toEqual({ body: `x`, fromResults: false })
    state.row = { ...baseRow, results: [{ topic: `nav`, label: `web`, attachmentId: `a1` }] }
    expect(await runPrBody(`s1`, `x`)).toEqual({ body: `x`, fromResults: false })
    state.selectThrows = true
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    expect(await runPrBody(`s1`, undefined)).toEqual({ body: undefined, fromResults: false })
    warn.mockRestore()
  })
})

describe(`syncRunPrBody`, () => {
  it(`patches the open PR's body`, async () => {
    expect(await syncRunPrBody(`s1`)).toBe(`synced`)
    const call = state.patch.mock.calls[0]![0] as {
      repoFullName: string
      prNumber: number
      token: string
      fields: { body?: string; title?: string }
    }
    expect(call.repoFullName).toBe(`acme/web`)
    expect(call.prNumber).toBe(7)
    expect(call.token).toBe(`tok`)
    expect(call.fields.title).toBeUndefined()
    expect(call.fields.body).toContain(`Did it`)
  })

  it(`skips without an open PR or report text`, async () => {
    expect(await syncRunPrBody(null)).toBe(`skipped`)
    state.row = { ...baseRow, prState: `merged` }
    expect(await syncRunPrBody(`s1`)).toBe(`skipped`)
    state.row = { ...baseRow, prUrl: null }
    expect(await syncRunPrBody(`s1`)).toBe(`skipped`)
    state.row = { ...baseRow, results: null }
    expect(await syncRunPrBody(`s1`)).toBe(`skipped`)
    state.row = null
    expect(await syncRunPrBody(`s1`)).toBe(`skipped`)
    expect(state.patch).not.toHaveBeenCalled()
  })

  it(`shrinks the body to the footer link when a removal took the last text`, async () => {
    state.row = { ...baseRow, results: [{ topic: `nav`, label: `web`, attachmentId: `a1` }] }
    expect(await syncRunPrBody(`s1`)).toBe(`skipped`)
    expect(state.patch).not.toHaveBeenCalled()
    expect(await syncRunPrBody(`s1`, { removal: true })).toBe(`synced`)
    expect((state.patch.mock.calls[0]![0] as { fields: { body: string } }).fields.body).toBe(
      `[Guide and screenshots in Exponential](https://exp.example/t/acme/boards/web/issues/EXP-1?view=guide)`
    )
    // A removal still needs an open PR.
    state.row = { ...baseRow, results: [], prState: `merged` }
    expect(await syncRunPrBody(`s1`, { removal: true })).toBe(`skipped`)
    expect(state.patch).toHaveBeenCalledTimes(1)
  })

  it(`never patches a PR other than the one the row was stamped with`, async () => {
    expect(
      await syncRunPrBody(`s1`, { expectPrUrl: `https://github.com/other/repo/pull/3` })
    ).toBe(`skipped`)
    expect(state.patch).not.toHaveBeenCalled()
    expect(await syncRunPrBody(`s1`, { expectPrUrl: baseRow.prUrl })).toBe(`synced`)
  })

  it(`serialises one run's syncs in call order and drains the chain`, async () => {
    const order: string[] = []
    const bodies = [`First`, `Second`]
    let release: () => void = () => {}
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    state.patch.mockImplementation(async (opts: unknown) => {
      const body = (opts as { fields: { body: string } }).fields.body
      order.push(`start:${body.split(`\n`)[0]}`)
      // The first PATCH is slow: the second must still wait for it.
      if (order.length === 1) await gate
      order.push(`end:${body.split(`\n`)[0]}`)
    })
    state.row = { ...baseRow, results: [{ ...report[0]!, text: bodies[0] }] }
    const first = syncRunPrBody(`s1`)
    // The second report write lands before the first PATCH finished.
    await Promise.resolve()
    const second = syncRunPrBody(`s1`)
    state.row = { ...baseRow, results: [{ ...report[0]!, text: bodies[1] }] }
    expect(pendingRunPrBodySyncs()).toBe(1)
    await new Promise((resolve) => setTimeout(resolve, 0))
    release()
    expect(await first).toBe(`synced`)
    expect(await second).toBe(`synced`)
    expect(order).toEqual([`start:First`, `end:First`, `start:Second`, `end:Second`])
    expect(pendingRunPrBodySyncs()).toBe(0)
  })

  it(`fails soft when GitHub refuses or the App is missing`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    state.patch.mockRejectedValueOnce(new Error(`422`))
    expect(await syncRunPrBody(`s1`)).toBe(`failed`)
    state.token = null
    expect(await syncRunPrBody(`s1`)).toBe(`failed`)
    warn.mockRestore()
  })
})

// EXP-1251: a run that stacked a second PR keeps each body to its own topics.
describe(`per-PR topics`, () => {
  const stacked = [
    { topic: `Summary`, label: null, attachmentId: null, text: `First PR`, prUrl: `https://github.com/acme/web/pull/6` },
    { topic: `Shared`, label: null, attachmentId: null, text: `Both` },
    { topic: `Second`, label: null, attachmentId: null, text: `Mine`, prUrl: `https://github.com/acme/web/pull/7` },
  ]

  it(`syncs the row's PR with its own and the untagged topics`, async () => {
    state.row = { ...baseRow, results: stacked }
    expect(await syncRunPrBody(`s1`)).toBe(`synced`)
    const body = (state.patch.mock.calls[0]![0] as { fields: { body: string } }).fields.body
    expect(body).toContain(`Mine`)
    expect(body).toContain(`Both`)
    expect(body).not.toContain(`First PR`)
  })

  it(`opens a new PR with the untagged topics only`, async () => {
    state.row = { ...baseRow, results: stacked }
    const out = await runPrBody(`s1`, `fallback`)
    expect(out.fromResults).toBe(true)
    expect(out.body).toContain(`Both`)
    expect(out.body).not.toContain(`Mine`)
    expect(out.body).not.toContain(`First PR`)
  })
})
