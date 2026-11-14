import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-1291: the pull-request API over the PR model. GitHub, the pr-sync state
// writers and the stack walks are mocked; the direct reads this module makes
// (root session, team slug, issues, chat links) drain a FIFO select queue.

const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const deleteRows: unknown[][] = []
  const chain = () => {
    const rows = selectQueue.shift() ?? []
    const builder: Record<string, unknown> = {}
    for (const m of [`from`, `where`, `innerJoin`, `leftJoin`, `orderBy`]) builder[m] = () => builder
    builder.limit = async () => rows
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    builder.then = (res: any, rej: any) => Promise.resolve(rows).then(res, rej)
    return builder
  }
  return {
    selectQueue,
    deleteRows,
    db: {
      select: vi.fn(chain),
      update: vi.fn(() => ({
        set: () => ({
          where: () => Object.assign(Promise.resolve(), { returning: async () => deleteRows.shift() ?? [] }),
        }),
      })),
      delete: vi.fn(() => ({ where: () => ({ returning: async () => deleteRows.shift() ?? [] }) })),
    },
  }
})

vi.mock(`@/db/connection`, () => ({ db: h.db }))
vi.mock(`@/lib/auth/access`, () => ({ resolveTeamAccess: vi.fn(async () => ({ kind: `member` })) }))
vi.mock(`@/lib/board-visibility`, () => ({ boardVisible: () => undefined, issueVisible: () => undefined }))
vi.mock(`@/lib/sessions/access`, () => ({ loadSessionForUser: vi.fn() }))
vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: () => `https://exp.example`,
  buildSessionDeepLinkPath: (team: string, id: string) => `/t/${team}/sessions/${id}`,
}))
vi.mock(`@/lib/integrations/github-pr`, () => ({
  closePullRequest: vi.fn(async () => {}),
  createPullRequest: vi.fn(async () => ({ url: `https://github.com/o/r/pull/7`, number: 7 })),
  findOpenPullByHead: vi.fn(async () => null),
  getPullRequest: vi.fn(),
  GitHubMergeError: class extends Error {
    constructor(
      public status: number,
      message: string
    ) {
      super(message)
    }
  },
  PullAlreadyExistsError: class extends Error {},
  retargetPullRequest: vi.fn(async () => {}),
  setPullRequestDraft: vi.fn(async () => {}),
  fetchPullStack: vi.fn(async () => ({ number: 3, size: 2, position: 1, baseRef: `master` })),
  unstack: vi.fn(async () => {}),
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoDefaultBranchCached: vi.fn(async () => `master`),
}))
vi.mock(`@/lib/integrations/pr-files`, () => ({
  loadPrFiles: vi.fn(async () => ({
    repo: `o/r`,
    prNumber: 7,
    files: [{ filename: `a.ts`, status: `modified`, additions: 2, deletions: 1, patch: `@@` }],
  })),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPullRequestBase: vi.fn(async () => ({ txid: 5 })),
  applyPullRequestClosed: vi.fn(async () => ({ applied: true, txid: 6 })),
  applyPullRequestDraft: vi.fn(async () => {}),
  applyPullRequestMerged: vi.fn(async () => ({ applied: true, txid: 7 })),
  livePullRequestsOfRepo: vi.fn(async () => []),
  livePullRequestsOfRoot: vi.fn(async () => []),
  loadLivePullRequestByHead: vi.fn(async () => null),
  loadPullRequest: vi.fn(),
  loadPullRequestByNumber: vi.fn(async () => null),
  loadPullRequestByUrl: vi.fn(async () => null),
  livePullRequestsForIssue: vi.fn(async () => []),
  newestLivePullRequestForSession: vi.fn(async () => null),
  recordPullRequestOpened: vi.fn(),
  repoFromPrUrl: (url: string) => url.match(/github\.com\/([^/]+\/[^/]+)\/pull/)?.[1] ?? null,
}))
vi.mock(`@/lib/pr-merge-guard`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pr-merge-guard")>()),
  loadGuardDefaultBranches: vi.fn(async () => ({
    defaultBranch: `master`,
    defaultBranchOverride: null,
    boardDefaultBranches: [],
  })),
}))
vi.mock(`@/lib/pr-stacks`, () => ({
  inferBaseBranch: vi.fn(async () => ({ base: `master`, prNumber: null })),
}))
vi.mock(`@/lib/trpc/pr-update`, () => ({ patchPullDescription: vi.fn(async () => {}) }))
vi.mock(`@/lib/sessions/pr-issues`, () => ({ prDefaultIssueIds: vi.fn(async () => []) }))
vi.mock(`@/lib/pr-pictures`, () => ({
  loadPrPictures: vi.fn(async () => []),
  compareCommitMessages: vi.fn(async () => []),
  linkedIssuesOfPullRequest: vi.fn(async () => []),
  chatUrlFor: (slug: string, rootId: string) => `https://exp.example/t/${slug}/sessions/${rootId}`,
}))
vi.mock(`@/lib/pr-ui-guard`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pr-ui-guard")>()),
  compareChangedPaths: vi.fn(async () => []),
}))
vi.mock(`@/lib/trpc/repositories`, () => ({
  effectiveDefaultBranch: (repo: { defaultBranch: string; defaultBranchOverride: string | null }) =>
    repo.defaultBranchOverride ?? repo.defaultBranch,
  loadRepository: vi.fn(async () => REPO),
  resolveGatedRepoToken: vi.fn(async () => `tok`),
}))

import { resolveTeamAccess } from "@/lib/auth/access"
import { loadSessionForUser } from "@/lib/sessions/access"
import * as gh from "@/lib/integrations/github-pr"
import * as sync from "@/lib/integrations/pr-sync"
import * as stacks from "@/lib/pr-stacks"
import { patchPullDescription } from "@/lib/trpc/pr-update"
import { compareChangedPaths } from "@/lib/pr-ui-guard"
import { compareCommitMessages, linkedIssuesOfPullRequest, loadPrPictures } from "@/lib/pr-pictures"
import { prDefaultIssueIds } from "@/lib/sessions/pr-issues"
import { _clearPrActorClaims, takePrOpenClaim } from "@/lib/integrations/pr-actor-claims"
import {
  closePullRequest,
  openPullRequest,
  pullRequestFiles,
  resolvePullRequest,
  syncPullRequestBodies,
  unlinkPullRequestIssue,
  retargetPullRequest,
} from "@/lib/pull-requests"

const REPO = {
  id: `repo-1`,
  teamId: `team-1`,
  fullName: `o/r`,
  defaultBranch: `master`,
  defaultBranchOverride: null,
  installationId: 1,
  inaccessibleAt: null,
  sharedByUserId: null,
  archivedAt: null,
}

const SESSION = {
  id: `sess-1`,
  rootId: `root-1`,
  teamId: `team-1`,
  repositoryId: `repo-1`,
  branch: `exp/chat-abcd1234`,
}

const FOOTER = `\n\n---\n\n[Results in Exponential](https://exp.example/t/acme/sessions/root-1)`

const pr = (over: Record<string, unknown> = {}) => ({
  id: `pr-1`,
  teamId: `team-1`,
  rootSessionId: `root-1`,
  sessionId: `sess-1`,
  repositoryId: `repo-1`,
  number: 7,
  url: `https://github.com/o/r/pull/7`,
  title: `Fix the nav`,
  head: `exp/EXP-1`,
  base: `master`,
  basePullRequestId: null,
  state: `open`,
  mergedAt: null,
  createdAt: new Date(),
  updatedAt: new Date(),
  ...over,
})

beforeEach(() => {
  vi.clearAllMocks()
  h.selectQueue.length = 0
  h.deleteRows.length = 0
  _clearPrActorClaims()
  vi.mocked(loadSessionForUser).mockResolvedValue(SESSION as never)
  vi.mocked(sync.recordPullRequestOpened).mockImplementation(async (args) => ({
    pullRequest: pr({ number: args.number, url: args.url, base: args.base }) as never,
    created: true,
    linkedIssueIds: args.issueIds,
    txid: 1,
  }))
  vi.mocked(sync.loadPullRequest).mockResolvedValue(pr() as never)
})

describe(`openPullRequest`, () => {
  it(`defaults repo, head and issues from the caller's chat; the body = the summary + pictures`, async () => {
    // EXP-1334: the session's share of the chat (pr-issues.ts).
    vi.mocked(prDefaultIssueIds).mockResolvedValueOnce([`i-1`, `i-2`])
    h.selectQueue.push(
      [{ id: `root-1`, teamId: `team-1` }], // the root
      [
        { id: `i-1`, identifier: `EXP-1`, title: `Nav`, teamId: `team-1`, boardRepositoryId: `repo-1`, boardDefaultBranch: null },
        // Another repository's issue: narrowed out of this PR, never refused.
        { id: `i-2`, identifier: `EXP-2`, title: `Other`, teamId: `team-1`, boardRepositoryId: `repo-9`, boardDefaultBranch: null },
      ],
      [{ slug: `acme` }] // the chat link
    )
    vi.mocked(loadPrPictures).mockResolvedValueOnce([
      { label: `web nav`, attachmentId: `a1`, url: `https://exp.example/api/attachments/a1` },
    ])
    vi.mocked(compareCommitMessages).mockResolvedValueOnce([`Fix the nav bar\n\nbody`])
    const result = await openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Fix the nav` })

    expect(prDefaultIssueIds).toHaveBeenCalledWith(expect.anything(), { rootId: `root-1`, sessionId: `sess-1` })
    expect(result).toEqual({
      pullRequestId: `pr-1`,
      url: `https://github.com/o/r/pull/7`,
      number: 7,
      base: `master`,
      head: `exp/chat-abcd1234`,
      // EXP-1377: the issues it linked (EXP-2 is another repository's).
      issues: [`EXP-1`],
    })
    expect(gh.createPullRequest).toHaveBeenCalledWith({
      repo: `o/r`,
      head: `exp/chat-abcd1234`,
      base: `master`,
      title: `Fix the nav`,
      body: `Fix the nav\n\n- #EXP-1 Nav\n- Fix the nav bar\n\n**web** · [web nav](https://exp.example/api/attachments/a1)${FOOTER}`,
      token: `tok`,
    })
    expect(loadPrPictures).toHaveBeenCalledWith(`root-1`, `sess-1`, { pullRequestId: null, head: `exp/chat-abcd1234`, repositoryId: `repo-1` })
    expect(sync.recordPullRequestOpened).toHaveBeenCalledWith(
      expect.objectContaining({
        teamId: `team-1`,
        rootSessionId: `root-1`,
        sessionId: `sess-1`,
        repositoryId: `repo-1`,
        head: `exp/chat-abcd1234`,
        base: `master`,
        issueIds: [`i-1`],
        actorUserId: `u-1`,
        actorViaAgent: true,
      })
    )
    // The `opened` webhook consumes the claim attributed to the opener.
    expect(takePrOpenClaim(`o/r`, `exp/chat-abcd1234`)).toEqual({ userId: `u-1`, viaAgent: true })
  })

  it(`infers the base over the repo's live PRs; the PR stacks on it (no native stack, EXP-1709)`, async () => {
    h.selectQueue.push([{ id: `root-1`, teamId: `team-1`, guide: null }], [{ slug: `acme` }])
    vi.mocked(sync.livePullRequestsOfRepo).mockResolvedValueOnce([
      pr({ id: `pr-0`, number: 5, head: `exp/EXP-0` }),
    ] as never)
    vi.mocked(stacks.inferBaseBranch).mockResolvedValueOnce({ base: `exp/EXP-0`, prNumber: 5 })
    vi.mocked(sync.recordPullRequestOpened).mockResolvedValueOnce({
      pullRequest: pr({ basePullRequestId: `pr-0`, base: `exp/EXP-0` }) as never,
      created: true,
      linkedIssueIds: [],
      txid: 1,
    })

    const result = await openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Stacked` })

    expect(stacks.inferBaseBranch).toHaveBeenCalledWith(
      expect.objectContaining({ head: `exp/chat-abcd1234`, candidates: [{ number: 5, branch: `exp/EXP-0` }] })
    )
    expect(result.base).toBe(`exp/EXP-0`)
  })

  it(`refuses an explicit issue of another team before any GitHub call`, async () => {
    h.selectQueue.push(
      [{ id: `root-1`, teamId: `team-1`, guide: null }],
      [{ id: `i-9`, identifier: `OPS-9`, teamId: `team-2`, boardRepositoryId: null, boardDefaultBranch: null }]
    )
    await expect(
      openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `x`, issueIds: [`i-9`] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: expect.stringMatching(/OPS-9/) })
    expect(gh.createPullRequest).not.toHaveBeenCalled()
  })

  // EXP-1315: a UI diff needs a labelled Guide picture per touched platform.
  const picture = (label: string) => ({
    label,
    attachmentId: `att-${label}`,
    url: `https://exp.example/api/attachments/att-${label}`,
  })
  const chatOf = (pictures: ReturnType<typeof picture>[]) => {
    h.selectQueue.push([{ id: `root-1`, teamId: `team-1` }], [{ slug: `acme` }])
    vi.mocked(loadPrPictures).mockResolvedValueOnce(pictures)
  }

  it(`refuses a UI diff without a labelled picture, before any GitHub write`, async () => {
    chatOf([picture(`web-mobile nav`)])
    vi.mocked(compareChangedPaths).mockResolvedValueOnce([
      `apps/web/src/components/nav.tsx`,
      `apps/ios/Sources/Nav.swift`,
      `README.md`,
    ])
    await expect(
      openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Nav` })
    ).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: expect.stringMatching(/changes UI on ios .*notRun/),
    })
    expect(compareChangedPaths).toHaveBeenCalledWith(
      expect.objectContaining({ repo: `o/r`, base: `master`, head: `exp/chat-abcd1234`, token: `tok` })
    )
    expect(gh.createPullRequest).not.toHaveBeenCalled()
    expect(takePrOpenClaim(`o/r`, `exp/chat-abcd1234`)).toBeNull()
  })

  it(`notRun is kept on the PR row, covers its platform and closes the body`, async () => {
    chatOf([picture(`web nav`)])
    vi.mocked(compareChangedPaths).mockResolvedValueOnce([
      `apps/web/src/components/nav.tsx`,
      `apps/android/app/src/main/Nav.kt`,
    ])
    await openPullRequest({
      userId: `u-1`,
      callerSessionId: `sess-1`,
      title: `Nav`,
      notRun: ` android:  the nav drawer `,
    })
    expect(gh.createPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({
        body: `Nav\n\n**web** · [web nav](https://exp.example/api/attachments/att-web nav)\n\n**Not run:** android: the nav drawer${FOOTER}`,
      })
    )
    expect(h.db.update).toHaveBeenCalled()
  })

  it(`a live PR's notRun covers a later pr_open without notRun`, async () => {
    chatOf([picture(`web nav`)])
    vi.mocked(sync.loadLivePullRequestByHead).mockResolvedValueOnce(
      pr({ head: `exp/chat-abcd1234`, notRun: `android: the nav drawer` }) as never
    )
    await openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Nav` })
    expect(patchPullDescription).toHaveBeenCalledWith(
      expect.objectContaining({ fields: expect.objectContaining({ body: expect.stringContaining(`**Not run:** android: the nav drawer`) }) })
    )
  })

  it(`a failed compare never blocks the PR`, async () => {
    chatOf([])
    vi.mocked(compareChangedPaths).mockRejectedValueOnce(new Error(`GitHub returned 502`))
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    await openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Nav` })
    expect(gh.createPullRequest).toHaveBeenCalled()
    warn.mockRestore()
  })

  // EXP-1468: a sealed show card (device gateway) feeds the gate through
  // its clear part: uploaded pictures count, result:false and pending don't.
  const sealedChat = async (cards: Record<string, unknown>[]) => {
    const { prPicturesOfCard } = await vi.importActual<typeof import("@/lib/pr-pictures")>(`@/lib/pr-pictures`)
    chatOf(
      cards.flatMap((card) => prPicturesOfCard(card)).map((p) => ({ ...p, url: `https://exp.example/api/attachments/${p.attachmentId}` }))
    )
  }
  const sealedSurface = (extra: Record<string, unknown>) => ({ kind: `surface`, sealed: `v1:1:${`A`.repeat(80)}`, ...extra })

  it(`a sealed card's clear pictures pass the UI gate and land in the body by label`, async () => {
    await sealedChat([sealedSurface({ pictures: [{ label: `web nav`, attachmentId: `att-sealed` }] })])
    vi.mocked(compareChangedPaths).mockResolvedValueOnce([`apps/web/src/components/nav.tsx`])
    await openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Nav` })
    expect(gh.createPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ body: `Nav\n\n**web** · [web nav](https://exp.example/api/attachments/att-sealed)${FOOTER}` })
    )
  })

  it(`a sealed result:false card or a picture still to upload never passes the UI gate`, async () => {
    await sealedChat([
      sealedSurface({ result: false, pictures: [{ label: `web nav`, attachmentId: `att-chat` }] }),
      sealedSurface({ result: true, pictures: [{ label: `web nav` }] }),
    ])
    vi.mocked(compareChangedPaths).mockResolvedValueOnce([`apps/web/src/components/nav.tsx`])
    await expect(openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Nav` })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: expect.stringMatching(/changes UI on web/),
    })
    expect(gh.createPullRequest).not.toHaveBeenCalled()
  })

  it(`needs a head without a session`, async () => {
    await expect(
      openPullRequest({ userId: `u-1`, callerSessionId: null, repositoryId: `repo-1`, title: `x` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`#NEW-20: a defaulted head never takes over another session's PR`, async () => {
    h.selectQueue.push(
      [{ id: `root-1`, teamId: `team-1` }],
      [{ id: `sess-1`, parentId: `root-1`, continuedFromId: null, status: `running`, branch: `exp/chat-abcd1234` }]
    )
    vi.mocked(sync.loadLivePullRequestByHead).mockResolvedValueOnce(
      pr({ head: `exp/chat-abcd1234`, sessionId: `sess-parent` }) as never
    )
    await expect(openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Other` })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: expect.stringMatching(/head of PR #7, opened by another session.*'head'/),
    })
    expect(patchPullDescription).not.toHaveBeenCalled()
    expect(sync.recordPullRequestOpened).not.toHaveBeenCalled()
  })

  it(`#NEW-20: a continuation keeps its ended predecessor's PR with a defaulted head`, async () => {
    h.selectQueue.push(
      [{ id: `root-1`, teamId: `team-1` }],
      [
        { id: `sess-old`, parentId: `root-1`, continuedFromId: null, status: `ended`, branch: `exp/chat-abcd1234` },
        { id: `sess-1`, parentId: `root-1`, continuedFromId: `sess-old`, status: `running`, branch: `exp/chat-abcd1234` },
      ],
      [{ slug: `acme` }]
    )
    vi.mocked(sync.loadLivePullRequestByHead).mockResolvedValueOnce(
      pr({ head: `exp/chat-abcd1234`, sessionId: `sess-old` }) as never
    )
    await openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, title: `Same` })
    expect(patchPullDescription).toHaveBeenCalledTimes(1)
  })

  it(`#NEW-20: a named head on another chat's PR updates it, but its body stays that chat's`, async () => {
    h.selectQueue.push([{ id: `root-1`, teamId: `team-1` }], [{ slug: `acme` }])
    vi.mocked(sync.loadLivePullRequestByHead).mockResolvedValueOnce(
      pr({ head: `exp/other`, sessionId: `sess-9`, rootSessionId: `root-9` }) as never
    )
    await openPullRequest({ userId: `u-1`, callerSessionId: `sess-1`, head: `exp/other`, title: `Follow-up` })
    expect(vi.mocked(patchPullDescription).mock.calls[0]![0].fields).toEqual({ title: `Follow-up` })
  })

  it(`is idempotent: a live PR on the head is updated and gains the issues`, async () => {
    h.selectQueue.push(
      [{ id: `root-1`, teamId: `team-1` }],
      [{ id: `i-1`, identifier: `EXP-1`, title: `Nav`, teamId: `team-1`, boardRepositoryId: `repo-1`, boardDefaultBranch: null }],
      [{ slug: `acme` }]
    )
    vi.mocked(sync.loadLivePullRequestByHead).mockResolvedValueOnce(pr({ head: `exp/chat-abcd1234` }) as never)

    const result = await openPullRequest({
      userId: `u-1`,
      callerSessionId: `sess-1`,
      title: `Renamed`,
      issueIds: [`i-1`],
    })

    expect(gh.createPullRequest).not.toHaveBeenCalled()
    expect(patchPullDescription).toHaveBeenCalledWith(
      expect.objectContaining({ prNumber: 7, fields: expect.objectContaining({ title: `Renamed` }) })
    )
    expect(sync.recordPullRequestOpened).toHaveBeenCalledWith(
      expect.objectContaining({ url: `https://github.com/o/r/pull/7`, issueIds: [`i-1`] })
    )
    expect(result.pullRequestId).toBe(`pr-1`)
  })
})

describe(`retargetPullRequest`, () => {
  it(`#EXP-1711: a stack member GitHub refuses leaves the stack, then moves`, async () => {
    h.selectQueue.push([])
    vi.mocked(gh.retargetPullRequest).mockRejectedValueOnce(
      new gh.GitHubMergeError(422, `Pull request is part of a stack`)
    )
    await expect(retargetPullRequest({ userId: `u-1`, pullRequestId: `pr-1`, base: `master` })).resolves.toEqual({ base: `master` })
    expect(gh.fetchPullStack).toHaveBeenCalledWith({ repo: `o/r`, prNumber: 7, token: `tok` })
    expect(gh.unstack).toHaveBeenCalledWith({ repo: `o/r`, stackNumber: 3, token: `tok` })
    expect(gh.retargetPullRequest).toHaveBeenCalledTimes(2)
    expect(sync.applyPullRequestBase).toHaveBeenCalledWith({ pullRequestId: `pr-1`, base: `master` })
  })

  it(`any other refusal stays a refusal`, async () => {
    h.selectQueue.push([])
    vi.mocked(gh.retargetPullRequest).mockRejectedValueOnce(new gh.GitHubMergeError(422, `No such base`))
    await expect(retargetPullRequest({ userId: `u-1`, pullRequestId: `pr-1`, base: `nope` })).rejects.toThrow(/not a valid base/)
    expect(gh.unstack).not.toHaveBeenCalled()
  })
})

describe(`resolvePullRequest`, () => {
  it(`an issue resolves to its one live PR`, async () => {
    h.selectQueue.push([{ teamId: `team-1` }])
    vi.mocked(sync.livePullRequestsForIssue).mockResolvedValueOnce([
      { pr: pr() as never, linkedIssues: 1, assigned: false },
    ])
    await expect(resolvePullRequest({ userId: `u-1`, issueId: `i-1` })).resolves.toMatchObject({ id: `pr-1` })
  })

  // FEED-111: EXP-1327's own PR #1092 lost to the newer chat-level #1096,
  // which links every root issue, so `pr_merge({issueId})` aimed at #1096.
  const CHAT_PR = { pr: pr({ id: `pr-1096`, number: 1096 }) as never, linkedIssues: 8, assigned: false }
  const OWN_PR = { pr: pr({ id: `pr-1092`, number: 1092 }) as never, linkedIssues: 1, assigned: false }

  it(`an issue with several live PRs resolves to the one focused on it, not the newest`, async () => {
    h.selectQueue.push([{ teamId: `team-1` }])
    vi.mocked(sync.livePullRequestsForIssue).mockResolvedValueOnce([CHAT_PR, OWN_PR])
    await expect(resolvePullRequest({ userId: `u-1`, issueId: `i-1` })).resolves.toMatchObject({
      id: `pr-1092`,
    })
  })

  it(`the PR the issue's assigned session opened wins`, async () => {
    h.selectQueue.push([{ teamId: `team-1` }])
    vi.mocked(sync.livePullRequestsForIssue).mockResolvedValueOnce([
      { ...CHAT_PR, assigned: true },
      OWN_PR,
    ])
    await expect(resolvePullRequest({ userId: `u-1`, issueId: `i-1` })).resolves.toMatchObject({
      id: `pr-1096`,
    })
  })

  it(`a tie refuses and names the candidates`, async () => {
    h.selectQueue.push([{ teamId: `team-1` }])
    vi.mocked(sync.livePullRequestsForIssue).mockResolvedValueOnce([{ ...CHAT_PR, linkedIssues: 1 }, OWN_PR])
    await expect(resolvePullRequest({ userId: `u-1`, issueId: `i-1` })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: expect.stringMatching(/#1096.*#1092/),
    })
  })

  it(`nothing named = the caller session's newest live PR`, async () => {
    vi.mocked(sync.newestLivePullRequestForSession).mockResolvedValueOnce(pr() as never)
    await resolvePullRequest({ userId: `u-1`, callerSessionId: `sess-1` })
    expect(sync.newestLivePullRequestForSession).toHaveBeenCalledWith(SESSION)
  })

  it(`a foreign team's PR is NOT_FOUND`, async () => {
    vi.mocked(resolveTeamAccess).mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(resolvePullRequest({ userId: `u-9`, pullRequestId: `pr-1` })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
  })
})

describe(`closePullRequest / pullRequestFiles`, () => {
  it(`closes on GitHub, then records it`, async () => {
    await closePullRequest({ userId: `u-1`, pullRequestId: `pr-1` })
    expect(gh.closePullRequest).toHaveBeenCalledWith({ repo: `o/r`, prNumber: 7, token: `tok` })
    expect(sync.applyPullRequestClosed).toHaveBeenCalledWith({ pullRequestId: `pr-1` })
  })

  it(`maps GitHub's files onto the Changes rows`, async () => {
    await expect(pullRequestFiles({ userId: `u-1`, pullRequestId: `pr-1` })).resolves.toEqual({
      files: [{ path: `a.ts`, additions: 2, deletions: 1, patch: `@@` }],
    })
  })
})

describe(`unlinkPullRequestIssue`, () => {
  it(`turns the one closing pull_request_issues row into a reference and re-renders the live PR's body (EXP-1706)`, async () => {
    h.deleteRows.push([{ issueId: `issue-2` }])
    h.selectQueue.push([{ id: `root-1`, teamId: `team-1` }])
    await expect(
      unlinkPullRequestIssue({ userId: `u-1`, pullRequestId: `pr-1`, issueId: `issue-2` })
    ).resolves.toEqual({ unlinked: true })
    expect(h.db.update).toHaveBeenCalledTimes(1)
    expect(h.db.delete).not.toHaveBeenCalled()
    await vi.waitFor(() => expect(sync.livePullRequestsOfRoot).toHaveBeenCalledWith(`root-1`))
  })

  it(`#FEED-236: unlinking a reference again answers unlinked (already), never unlinked: false`, async () => {
    h.deleteRows.push([])
    h.selectQueue.push([{ issueId: `issue-2` }])
    await expect(
      unlinkPullRequestIssue({ userId: `u-1`, pullRequestId: `pr-1`, issueId: `issue-2` })
    ).resolves.toEqual({ unlinked: true, already: true })
    expect(sync.livePullRequestsOfRoot).not.toHaveBeenCalled()
  })

  it(`#FEED-236: an issue the PR never linked errors, saying so`, async () => {
    h.deleteRows.push([])
    h.selectQueue.push([])
    await expect(
      unlinkPullRequestIssue({ userId: `u-1`, pullRequestId: `pr-1`, issueId: `issue-9` })
    ).rejects.toMatchObject({ code: `NOT_FOUND`, message: expect.stringContaining(`not linked to pull request #7`) })
  })

  it(`a foreign team's PR is NOT_FOUND and touches nothing`, async () => {
    vi.mocked(resolveTeamAccess).mockRejectedValueOnce(new Error(`no`))
    await expect(
      unlinkPullRequestIssue({ userId: `u-1`, pullRequestId: `pr-1`, issueId: `issue-2` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(h.db.update).not.toHaveBeenCalled()
  })
})

describe(`syncPullRequestBodies`, () => {
  it(`re-renders each live PR's body from its opener's pictures, issues and notRun`, async () => {
    h.selectQueue.push([{ id: `root-1`, teamId: `team-1` }], [{ slug: `acme` }])
    vi.mocked(sync.livePullRequestsOfRoot).mockResolvedValueOnce([
      pr({ notRun: null }),
      pr({ id: `pr-3`, number: 3, url: `https://github.com/o/r/pull/3`, title: `Other`, sessionId: `sess-3`, notRun: `ios: chats` }),
    ] as never)
    vi.mocked(linkedIssuesOfPullRequest).mockResolvedValueOnce([{ identifier: `EXP-1`, title: `Nav` }])
    vi.mocked(loadPrPictures).mockImplementation(async (_root, sessionId) =>
      sessionId === `sess-1` ? [{ label: `web nav`, attachmentId: `a1`, url: `https://x/a1` }] : []
    )
    await syncPullRequestBodies(`root-1`)
    const bodies = vi.mocked(patchPullDescription).mock.calls.map(([call]) => [call.prNumber, call.fields.body])
    expect(bodies).toEqual([
      [7, `Fix the nav\n\n- #EXP-1 Nav\n\n**web** · [web nav](https://x/a1)${FOOTER}`],
      [3, `Other\n\n**Not run:** ios: chats${FOOTER}`],
    ])
    vi.mocked(loadPrPictures).mockReset()
  })

  it(`never throws`, async () => {
    h.db.select.mockImplementationOnce(() => {
      throw new Error(`db down`)
    })
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    await expect(syncPullRequestBodies(`root-1`)).resolves.toBeUndefined()
    warn.mockRestore()
  })
})
