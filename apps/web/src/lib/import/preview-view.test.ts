import { describe, expect, it } from "vitest"
import type { ImportPlan, ImportPreview } from "@/lib/import/bundle"
import { buildDefaultPlan } from "@/lib/import/plan"
import {
  groupPreviewStatuses,
  isVisible,
  visibleBoardKeys,
  visiblePreviewLabels,
  visiblePreviewUsers,
} from "@/lib/import/preview-view"
import { linearPreview } from "@/lib/import/linear/bundle"
import {
  linearSnapshotFixture,
  P_MAINT,
  T_MET,
  T_SOV,
  teamStateFixture,
  U_DENNIS,
  U_HANNES,
} from "@/lib/import/fixtures"

// EXP-1076: the wizard's read model over a preview — which rows a plan's
// boards still justify, and the (name, category) collapse that turns each
// Linear team's own "Todo" into ONE decision.

// The shared fixture plus a second team that has issues of its own: a `Todo`
// state both teams spell the same way, assigned to Hannes.
function previewFixtureWithSov(): ImportPreview {
  const snapshot = linearSnapshotFixture()
  snapshot.states.push({
    id: `st-sov-todo`,
    name: `Todo`,
    type: `unstarted`,
    color: `#e2e2e2`,
    position: 1,
    teamId: T_SOV,
  })
  snapshot.issues.push({
    ...snapshot.issues[2]!,
    id: `is-5`,
    teamId: T_SOV,
    identifier: `SOV-1`,
    number: 1,
    title: `Sovereign work`,
    stateId: `st-sov-todo`,
    assigneeId: U_HANNES,
    creatorId: U_HANNES,
    projectId: null,
    parentId: null,
    labelIds: [`lb-bug`],
    history: [],
  })
  return linearPreview(snapshot)
}

const preview = previewFixtureWithSov()
const plan = buildDefaultPlan(preview, teamStateFixture())
const teamNames = new Map(preview.teams.map((team) => [team.key, team.name]))
const groupOf = (groups: ReturnType<typeof groupPreviewStatuses>, name: string) =>
  groups.filter((group) => group.name.toLowerCase() === name.toLowerCase())

function withBoards(overrides: ImportPlan[`boards`], rest: Partial<ImportPlan> = {}): ImportPlan {
  return { ...plan, ...rest, boards: { ...plan.boards, ...overrides } }
}

describe(`visibleBoardKeys`, () => {
  it(`takes every non-skipped board and ignores project entries under team routing`, () => {
    expect([...visibleBoardKeys(preview, plan)].sort()).toEqual([
      `archive:${T_MET}`,
      `team:${T_MET}`,
      `team:${T_SOV}`,
    ])
  })

  it(`drops the archive boards when archived issues are left out`, () => {
    const keys = visibleBoardKeys(preview, { ...plan, importArchived: false })
    expect(keys.has(`archive:${T_MET}`)).toBe(false)
    expect(keys.has(`team:${T_MET}`)).toBe(true)
  })

  it(`resolves a project board onto the team its counts are keyed by`, () => {
    const projectRouted = withBoards(
      { [`team:${T_MET}`]: { mode: `skip` } },
      { routing: `project` }
    )
    const keys = visibleBoardKeys(preview, projectRouted)
    // The team board is skipped, but its project still brings MET issues in.
    expect(plan.boards[`project:${P_MAINT}`]?.mode).toBe(`create`)
    expect(keys.has(`team:${T_MET}`)).toBe(true)
  })
})

describe(`isVisible`, () => {
  const visible = new Set([`team:a`])
  it(`hides only a present, non-empty map with no hit on a visible board`, () => {
    expect(isVisible({ [`team:b`]: 3 }, visible)).toBe(false)
    expect(isVisible({ [`team:a`]: 1, [`team:b`]: 3 }, visible)).toBe(true)
    expect(isVisible({ [`team:a`]: 0 }, visible)).toBe(false)
    // An old preview carries no map at all: never hide it. A present but
    // EMPTY map is a state nothing references — hidden.
    expect(isVisible(undefined, visible)).toBe(true)
    expect(isVisible({}, visible)).toBe(false)
  })
})

describe(`groupPreviewStatuses`, () => {
  const visible = visibleBoardKeys(preview, plan)

  it(`collapses the same name + category across teams into one row`, () => {
    const groups = groupPreviewStatuses(preview.statuses, visible, teamNames)
    const todo = groupOf(groups, `Todo`)
    expect(todo).toHaveLength(1)
    expect(todo[0]!.keys).toEqual([`state:st-todo`, `state:st-sov-todo`])
    // The hint names only the teams whose Todo is referenced on a visible
    // board — the fixture's Methode 5 Todo holds no issue.
    expect(todo[0]!.teamNames).toEqual([`soverius-ai`])
    expect(todo[0]!.id).toBe(`todo|unstarted`)
    // Both teams' Backlog holds no issue: the unused default state is noise
    // the wizard never shows (its keys still get the default plan's entry).
    expect(groupOf(groups, `Backlog`)).toHaveLength(0)
  })

  it(`never merges two categories, and orders by category then name`, () => {
    const todo = preview.statuses.find((status) => status.key === `state:st-sov-todo`)!
    const groups = groupPreviewStatuses(
      [...preview.statuses, { ...todo, key: `state:st-todo-started`, category: `started` }],
      visible,
      teamNames
    )
    expect(groupOf(groups, `Todo`).map((group) => group.category)).toEqual([`unstarted`, `started`])
    const categories = groups.map((group) => group.category)
    expect(categories).toEqual([...categories].sort((left, right) => {
      const order = [`backlog`, `unstarted`, `started`, `completed`, `cancelled`, `duplicate`]
      return order.indexOf(left) - order.indexOf(right)
    }))
  })

  it(`sums the issue counts over the visible boards only`, () => {
    const done = groupOf(groupPreviewStatuses(preview.statuses, visible, teamNames), `Done`)[0]!
    // One live MET issue and the archived one.
    expect(done.issueCount).toBe(2)
    const live = visibleBoardKeys(preview, { ...plan, importArchived: false })
    expect(groupOf(groupPreviewStatuses(preview.statuses, live, teamNames), `Done`)[0]!.issueCount).toBe(1)
  })

  it(`shows a status whose preview carries no map at all`, () => {
    const stripped = preview.statuses.map(({ issueCountByBoard: _drop, ...status }) => status)
    expect(groupPreviewStatuses(stripped, new Set(), teamNames).length).toBeGreaterThan(0)
  })
})

describe(`skipping a team`, () => {
  const withoutMet = withBoards({
    [`team:${T_MET}`]: { mode: `skip` },
    [`archive:${T_MET}`]: { mode: `skip` },
  })
  const visible = visibleBoardKeys(preview, withoutMet)

  it(`hides the statuses, labels and people that only lived there`, () => {
    const groups = groupPreviewStatuses(preview.statuses, visible, teamNames)
    expect(groupOf(groups, `Duplicate`)).toHaveLength(0)
    expect(groupOf(groups, `Done`)).toHaveLength(0)
    // Todo survives through the second team's own state.
    expect(groupOf(groups, `Todo`)[0]!.keys).toEqual([`state:st-todo`, `state:st-sov-todo`])

    const labels = visiblePreviewLabels(preview.labels, visible).map((label) => label.key)
    expect(labels).not.toContain(`label:lb-ios`)
    // The workspace label is on the surviving team's issue too.
    expect(labels).toContain(`label:lb-bug`)

    const users = visiblePreviewUsers(preview.users, visible).map((user) => user.key)
    expect(users).toContain(`user:${U_HANNES}`)
    expect(users).not.toContain(`user:${U_DENNIS}`)
  })

  it(`keeps everything when the preview has no maps (an older job)`, () => {
    const users = preview.users.map(
      ({ issueCountByBoard: _issues, commentCountByBoard: _comments, ...user }) => user
    )
    const labels = preview.labels.map(({ issueCountByBoard: _drop, ...label }) => label)
    expect(visiblePreviewUsers(users, new Set()).map((user) => user.key)).toEqual(
      preview.users.map((user) => user.key)
    )
    expect(visiblePreviewLabels(labels, new Set())).toHaveLength(preview.labels.length)
  })
})

describe(`visiblePreviewUsers`, () => {
  it(`keeps a commenter whose issues are all gone`, () => {
    const visible = new Set([`team:x`])
    const users: ImportPreview[`users`] = [
      {
        key: `user:commenter`,
        name: `Commenter`,
        email: `c@example.com`,
        active: true,
        issueCount: 0,
        commentCount: 1,
        issueCountByBoard: { [`team:y`]: 4 },
        commentCountByBoard: { [`team:x`]: 1 },
      },
      {
        key: `user:elsewhere`,
        name: `Elsewhere`,
        email: `e@example.com`,
        active: true,
        issueCount: 4,
        commentCount: 0,
        issueCountByBoard: { [`team:y`]: 4 },
        commentCountByBoard: {},
      },
    ]
    expect(visiblePreviewUsers(users, visible).map((user) => user.key)).toEqual([`user:commenter`])
  })
})
