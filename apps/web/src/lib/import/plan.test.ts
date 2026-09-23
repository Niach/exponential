import { describe, expect, it } from "vitest"
import { toLinearBundle } from "@/lib/import/linear/bundle"
import { buildDefaultPlan, derivePrefix, evaluatePlan } from "@/lib/import/plan"
import { BOARD_PREFIX_PATTERN, type ImportPlan } from "@/lib/import/bundle"
import {
  linearSnapshotFixture,
  P_MAINT,
  previewFixture,
  T_MET,
  T_SOV,
  teamStateFixture,
  U_BOT,
  U_DENNIS,
  U_HANNES,
} from "@/lib/import/fixtures"

// EXP-630: the Map step's auto-match and the dry run's rules, both pure.

describe(`buildDefaultPlan`, () => {
  const plan = buildDefaultPlan(previewFixture(), teamStateFixture())

  it(`creates a board per Linear team with issues, skips empty teams, derives project prefixes`, () => {
    expect(plan.boards[`team:${T_MET}`]).toEqual({
      mode: `create`,
      name: `Methode 5`,
      prefix: `MET`,
      numbering: `preserve`,
    })
    expect(plan.boards[`team:${T_SOV}`]).toEqual({ mode: `skip` })
    const project = plan.boards[`project:${P_MAINT}`]!
    expect(project.mode).toBe(`create`)
    if (project.mode === `create`) {
      expect(project.prefix).toMatch(BOARD_PREFIX_PATTERN)
      expect(project.prefix).not.toBe(`MET`)
    }
  })

  it(`reuses an existing board with the same prefix, allocating numbers when it has issues`, () => {
    const withBoard = buildDefaultPlan(
      previewFixture(),
      teamStateFixture({
        boards: [{ id: `board-met`, name: `Old MET`, prefix: `met`, issueCount: 12 }],
      })
    )
    expect(withBoard.boards[`team:${T_MET}`]).toEqual({
      mode: `existing`,
      boardId: `board-met`,
      numbering: `allocate`,
    })
  })

  it(`matches statuses onto builtins by category + name (Canceled ≈ Cancelled) and creates the rest`, () => {
    expect(plan.statuses[`state:st-backlog`]).toEqual({ mode: `builtin`, builtinKey: `backlog` })
    expect(plan.statuses[`state:st-progress`]).toEqual({ mode: `builtin`, builtinKey: `in_progress` })
    expect(plan.statuses[`state:st-review`]).toEqual({ mode: `builtin`, builtinKey: `in_review` })
    expect(plan.statuses[`state:st-done`]).toEqual({ mode: `builtin`, builtinKey: `done` })
    expect(plan.statuses[`state:st-canceled`]).toEqual({ mode: `builtin`, builtinKey: `cancelled` })
    expect(plan.statuses[`state:st-dup`]).toEqual({ mode: `builtin`, builtinKey: `duplicate` })
    expect(plan.statuses[`state:st-icebox`]).toMatchObject({ mode: `create`, name: `Icebox`, category: `unstarted` })
    expect(plan.statuses[`state:st-rueck`]).toMatchObject({ mode: `create`, name: `Rückfrage`, category: `started` })
    expect(plan.statuses[`state:st-nicht`]).toMatchObject({
      mode: `create`,
      name: `Nicht reproduzierbar`,
      category: `cancelled`,
    })
    // A second team's Backlog lands on the same builtin.
    expect(plan.statuses[`state:st-sov-backlog`]).toEqual({ mode: `builtin`, builtinKey: `backlog` })
  })

  it(`reuses an existing custom status by name within its category`, () => {
    const withCustom = buildDefaultPlan(
      previewFixture(),
      teamStateFixture({
        statuses: [
          ...teamStateFixture().statuses,
          { id: `s-icebox`, name: `icebox`, category: `unstarted`, builtinKey: null, color: `#000000` },
        ],
      })
    )
    expect(withCustom.statuses[`state:st-icebox`]).toEqual({ mode: `existing`, statusId: `s-icebox` })
  })

  it(`reuses labels by case-insensitive name`, () => {
    expect(plan.labels[`label:lb-bug`]).toEqual({ mode: `existing`, labelId: `label-bug` })
    expect(plan.labels[`label:lb-ios`]).toEqual({ mode: `create` })
  })

  it(`auto-matches members by case-insensitive email and falls back to self`, () => {
    expect(plan.users[`user:${U_HANNES}`]).toEqual({ mode: `member`, userId: `member-hannes` })
    // dennis@ (Linear) vs danny@ (Exponential): the manual override is the feature.
    expect(plan.users[`user:${U_DENNIS}`]).toEqual({ mode: `self` })
    expect(plan.users[`user:${U_BOT}`]).toEqual({ mode: `self` })
    expect(plan.routing).toBe(`team`)
    expect(plan.importHistory).toBe(true)
  })
})

describe(`derivePrefix`, () => {
  it(`takes initials, letter-led, at most four chars, avoiding taken ones`, () => {
    expect(derivePrefix(`M5 - Maintenance`, new Set())).toBe(`MM`)
    expect(derivePrefix(`M5 Design Launch`, new Set([`MDL`]))).toBe(`M5DE`)
    expect(derivePrefix(`123 only digits`, new Set())).toMatch(BOARD_PREFIX_PATTERN)
  })
})

describe(`evaluatePlan`, () => {
  const bundle = toLinearBundle(linearSnapshotFixture(), { routing: `team` })
  const state = teamStateFixture()
  const plan = buildDefaultPlan(previewFixture(), state)

  it(`passes the default plan and counts only what a non-skipped issue references`, () => {
    const result = evaluatePlan(bundle, plan, state)
    expect(result.blockers).toEqual([])
    // Icebox (an issue) and Todo (history) are referenced; Rückfrage and
    // Nicht reproduzierbar are not.
    expect(result.counts).toMatchObject({
      boardsToCreate: 1,
      statusesToCreate: 2,
      labelsToCreate: 2,
      issues: 3,
      comments: 3,
      attachments: 2,
      assetBytes: 85_285,
      events: 5,
      invites: 0,
      skippedIssues: 0,
    })
    expect(result.warnings.join(`\n`)).toMatch(/attributed to you/)
  })

  it(`counts one fewer status to create without history`, () => {
    const result = evaluatePlan(bundle, { ...plan, importHistory: false }, state)
    expect(result.counts.statusesToCreate).toBe(1)
    expect(result.counts.events).toBe(0)
  })

  it(`blocks when the started cap would be exceeded`, () => {
    const full = teamStateFixture({
      statuses: [
        ...state.statuses,
        { id: `s-a`, name: `A`, category: `started`, builtinKey: null, color: `#000000` },
        { id: `s-b`, name: `B`, category: `started`, builtinKey: null, color: `#000000` },
      ],
    })
    // Route an issue through Rückfrage so it is referenced.
    const routed = { ...bundle, issues: bundle.issues.map((issue) => ({ ...issue, statusKey: `state:st-rueck` })) }
    const result = evaluatePlan(routed, plan, full)
    expect(result.blockers.join(`\n`)).toMatch(/at most 4 started statuses/)
  })

  it(`blocks a prefix that collides with an existing board or another created board`, () => {
    const withBoard = teamStateFixture({
      boards: [{ id: `board-x`, name: `X`, prefix: `MET`, issueCount: 0 }],
    })
    expect(evaluatePlan(bundle, plan, withBoard).blockers.join(`\n`)).toMatch(/already used by a board/)
    const clash: ImportPlan = {
      ...plan,
      boards: {
        ...plan.boards,
        [`team:${T_SOV}`]: { mode: `create`, name: `Sov`, prefix: `MET`, numbering: `preserve` },
      },
    }
    const withSov = { ...bundle, issues: [...bundle.issues, { ...bundle.issues[0]!, key: `issue:sov`, boardKey: `team:${T_SOV}` }] }
    expect(evaluatePlan(withSov, clash, state).blockers.join(`\n`)).toMatch(/used for both/)
  })

  it(`blocks preserved numbers that overlap the target board, unless numbering allocates`, () => {
    const withBoard = teamStateFixture({
      boards: [{ id: `board-met`, name: `MET`, prefix: `MET`, issueCount: 1, numbers: [2, 99] }],
    })
    const existing: ImportPlan = {
      ...plan,
      boards: {
        ...plan.boards,
        [`team:${T_MET}`]: { mode: `existing`, boardId: `board-met`, numbering: `preserve` },
      },
    }
    expect(evaluatePlan(bundle, existing, withBoard).blockers.join(`\n`)).toMatch(/1 issue number\(s\)/)
    const allocate: ImportPlan = {
      ...existing,
      boards: {
        ...existing.boards,
        [`team:${T_MET}`]: { mode: `existing`, boardId: `board-met`, numbering: `allocate` },
      },
    }
    const result = evaluatePlan(bundle, allocate, withBoard)
    expect(result.blockers).toEqual([])
    expect(result.warnings.join(`\n`)).toMatch(/get new numbers/)
  })

  it(`blocks an invite the seat gate would refuse and counts one otherwise`, () => {
    const invite: ImportPlan = {
      ...plan,
      users: { ...plan.users, [`user:${U_DENNIS}`]: { mode: `invite` } },
    }
    expect(evaluatePlan(bundle, invite, teamStateFixture({ canInvite: false })).blockers.join(`\n`)).toMatch(
      /exceed the team's seats/
    )
    expect(evaluatePlan(bundle, invite, state).counts.invites).toBe(1)
    expect(
      evaluatePlan(bundle, invite, teamStateFixture({ pendingInviteEmails: [`Dennis@straehhuber.com`] })).counts
        .invites
    ).toBe(0)
  })

  it(`blocks a storage overflow on a limited plan`, () => {
    const tight = teamStateFixture({ storage: { limitBytes: 100_000, usedBytes: 50_000 } })
    expect(evaluatePlan(bundle, plan, tight).blockers.join(`\n`)).toMatch(/storage/)
    const roomy = teamStateFixture({ storage: { limitBytes: 1_000_000, usedBytes: 50_000 } })
    expect(evaluatePlan(bundle, plan, roomy).blockers).toEqual([])
  })

  it(`skips a board's issues (and its unreferenced statuses) and reports already-imported ones`, () => {
    const skipped: ImportPlan = { ...plan, boards: { ...plan.boards, [`team:${T_MET}`]: { mode: `skip` } } }
    const result = evaluatePlan(bundle, skipped, state)
    expect(result.blockers).toEqual([])
    expect(result.counts).toMatchObject({ issues: 0, skippedIssues: 3, statusesToCreate: 0 })

    const resumed = evaluatePlan(
      bundle,
      plan,
      teamStateFixture({ importedIssueKeys: new Set([`issue:is-1`]) })
    )
    expect(resumed.counts).toMatchObject({ issues: 2, alreadyImported: 1 })

    // Everything imported already: no board is created, whatever the plan says.
    const done = evaluatePlan(
      bundle,
      plan,
      teamStateFixture({ importedIssueKeys: new Set(bundle.issues.map((issue) => issue.key)) })
    )
    expect(done.counts).toMatchObject({ issues: 0, alreadyImported: 3, boardsToCreate: 0, statusesToCreate: 0 })
  })

  it(`blocks a missing decision and a custom status in the duplicate category`, () => {
    const missing: ImportPlan = { ...plan, statuses: { ...plan.statuses } }
    delete missing.statuses[`state:st-done`]
    expect(evaluatePlan(bundle, missing, state).blockers.join(`\n`)).toMatch(/No target status chosen for "Done"/)
    const dup: ImportPlan = {
      ...plan,
      statuses: {
        ...plan.statuses,
        [`state:st-dup`]: { mode: `create`, name: `Dup`, color: `#000000`, category: `duplicate` },
      },
    }
    expect(evaluatePlan(bundle, dup, state).blockers.join(`\n`)).toMatch(/duplicate category takes no custom/)
  })

  it(`lets two created statuses share a name in the same category, blocks across categories`, () => {
    const shared: ImportPlan = {
      ...plan,
      statuses: {
        ...plan.statuses,
        [`state:st-icebox`]: { mode: `create`, name: `Later`, color: `#000000`, category: `unstarted` },
        [`state:st-todo`]: { mode: `create`, name: `later`, color: `#000000`, category: `unstarted` },
      },
    }
    const ok = evaluatePlan(bundle, shared, state)
    expect(ok.blockers).toEqual([])
    expect(ok.counts.statusesToCreate).toBe(1)
    const crossed: ImportPlan = {
      ...shared,
      statuses: {
        ...shared.statuses,
        [`state:st-todo`]: { mode: `create`, name: `later`, color: `#000000`, category: `started` },
      },
    }
    expect(evaluatePlan(bundle, crossed, state).blockers.join(`\n`)).toMatch(/created twice with different categories/)
  })
})
