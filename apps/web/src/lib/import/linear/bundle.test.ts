import { describe, expect, it } from "vitest"
import { importBundleSchema } from "@/lib/import/bundle"
import { linearPreview, toLinearBundle } from "@/lib/import/linear/bundle"
import {
  linearSnapshotFixture,
  P_MAINT,
  T_MET,
  T_SOV,
  U_BOT,
  UPLOAD_URL,
  UPLOAD_URL_2,
} from "@/lib/import/fixtures"

// EXP-630: the Linear adapter's one pure step — snapshot → bundle. The two
// routing rules, the state-type → category map, the duplicate rule, asset
// extraction, comment threading and history → events all live here.

describe(`toLinearBundle (team routing)`, () => {
  const bundle = toLinearBundle(linearSnapshotFixture(), { routing: `team` })

  it(`is a valid ImportBundle`, () => {
    expect(() => importBundleSchema.parse(bundle)).not.toThrow()
    expect(bundle.source).toBe(`linear`)
    expect(bundle.sourceLabel).toBe(`Linear`)
  })

  it(`routes every issue to its team's board and turns the project into a label`, () => {
    expect(bundle.boards.map((board) => board.key)).toEqual([
      `team:${T_MET}`,
      `team:${T_SOV}`,
      `archive:${T_MET}`,
    ])
    expect(bundle.boards[0]).toMatchObject({ name: `Methode 5`, prefix: `MET` })
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.boardKey).toBe(`team:${T_MET}`)
    expect(first.labelKeys).toContain(`project:${P_MAINT}`)
    expect(bundle.labels.find((label) => label.key === `project:${P_MAINT}`)).toMatchObject({
      name: `M5 - Maintenance`,
    })
  })

  it(`maps state types onto our categories and flags the duplicate state`, () => {
    const byKey = new Map(bundle.statuses.map((status) => [status.key, status]))
    expect(byKey.get(`state:st-backlog`)?.category).toBe(`backlog`)
    expect(byKey.get(`state:st-icebox`)?.category).toBe(`unstarted`)
    expect(byKey.get(`state:st-rueck`)?.category).toBe(`started`)
    expect(byKey.get(`state:st-done`)?.category).toBe(`completed`)
    expect(byKey.get(`state:st-canceled`)?.category).toBe(`cancelled`)
    expect(byKey.get(`state:st-dup`)).toMatchObject({ category: `duplicate`, builtinKey: `duplicate` })
  })

  it(`flattens label groups and blanks the Linear bot's email`, () => {
    expect(bundle.labels.find((label) => label.key === `label:lb-ios`)?.name).toBe(`Area/ios`)
    expect(bundle.users.find((user) => user.key === `user:${U_BOT}`)?.email).toBeNull()
  })

  it(`maps priorities, keeps timestamps and derives completedAt from canceledAt`, () => {
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.priority).toBe(`urgent`)
    expect(first.createdAt).toBe(`2025-01-01T10:00:00.000Z`)
    expect(first.completedAt).toBe(`2025-06-01T10:00:00.000Z`)
    expect(first.externalRef).toBe(`MET-1`)
    expect(first.externalUrl).toBe(`https://linear.app/methode5/issue/MET-1`)
    const second = bundle.issues.find((issue) => issue.key === `issue:is-2`)!
    expect(second.completedAt).toBe(`2025-02-02T10:00:00.000Z`)
    expect(second.dueDate).toBe(`2025-03-01`)
    expect(bundle.issues.find((issue) => issue.key === `issue:is-3`)!.priority).toBe(`low`)
  })

  it(`extracts upload URLs once per issue with their markdown names`, () => {
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.assets).toHaveLength(2)
    expect(first.assets[0]).toMatchObject({
      ref: UPLOAD_URL,
      filename: `shot.png`,
      sizeBytes: 85_285,
      commentKey: null,
    })
    expect(first.assets[1]).toMatchObject({ ref: UPLOAD_URL_2, filename: `file.pdf`, sizeBytes: null })
  })

  it(`attaches comments with their thread parents`, () => {
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.comments.map((comment) => [comment.key, comment.parentKey])).toEqual([
      [`comment:cm-1`, null],
      [`comment:cm-2`, `comment:cm-1`],
      [`comment:cm-3`, `comment:cm-2`],
    ])
    expect(first.comments[2]!.authorKey).toBeNull()
  })

  it(`turns history into the event kinds the timeline folds`, () => {
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.events.map((event) => event.type)).toEqual([
      `status_changed`,
      `assignee_changed`,
      `label_added`,
      `status_changed`,
      `priority_changed`,
    ])
    expect(first.events[4]).toMatchObject({ from: `medium`, to: `urgent` })
  })

  it(`writes duplicateOf only for a canceled/duplicate state and keeps blocks + related`, () => {
    const second = bundle.issues.find((issue) => issue.key === `issue:is-2`)!
    expect(second.duplicateOfKey).toBe(`issue:is-1`)
    const third = bundle.issues.find((issue) => issue.key === `issue:is-3`)!
    expect(third.blocksKeys).toEqual([`issue:is-1`])
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.relatedKeys).toEqual([`issue:is-3`])
  })

  it(`routes archived issues to the team's archive board, keeping the project as a label`, () => {
    expect(bundle.boards[2]).toEqual({
      key: `archive:${T_MET}`,
      name: `Methode 5 Archive`,
      prefix: `META`,
      icon: `archive`,
      archive: true,
    })
    const fourth = bundle.issues.find((issue) => issue.key === `issue:is-4`)!
    expect(fourth.boardKey).toBe(`archive:${T_MET}`)
    expect(fourth.archived).toBe(true)
    expect(fourth.labelKeys).toEqual([`label:lb-bug`, `project:${P_MAINT}`])
    // Linear's `similar` is our `related`.
    expect(fourth.relatedKeys).toEqual([`issue:is-1`])
  })

  it(`drops archived issues, their board and their relations with importArchived off`, () => {
    const live = toLinearBundle(linearSnapshotFixture(), { routing: `team`, importArchived: false })
    expect(live.boards.map((board) => board.key)).toEqual([`team:${T_MET}`, `team:${T_SOV}`])
    expect(live.issues.map((issue) => issue.key)).toEqual([`issue:is-1`, `issue:is-2`, `issue:is-3`])
  })

  it(`carries the parent link and the rounded estimate`, () => {
    const third = bundle.issues.find((issue) => issue.key === `issue:is-3`)!
    expect(third.parentKey).toBe(`issue:is-1`)
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.parentKey).toBeNull()
    expect(first.estimate).toBe(3)
    expect(bundle.issues.find((issue) => issue.key === `issue:is-4`)!.estimate).toBe(2)
    expect(bundle.issues.find((issue) => issue.key === `issue:is-2`)!.estimate).toBeNull()
  })

  it(`leaves a sub-issue a root when its parent is left out`, () => {
    const snapshot = linearSnapshotFixture()
    snapshot.issues[0]!.archivedAt = `2024-10-01T10:00:00.000Z`
    const live = toLinearBundle(snapshot, { routing: `team`, importArchived: false })
    expect(live.issues.find((issue) => issue.key === `issue:is-3`)!.parentKey).toBeNull()
  })

  it(`ignores a duplicate relation on an issue that is not canceled`, () => {
    const snapshot = linearSnapshotFixture()
    snapshot.issues[1]!.stateId = `st-progress`
    const rebuilt = toLinearBundle(snapshot, { routing: `team` })
    const second = rebuilt.issues.find((issue) => issue.key === `issue:is-2`)!
    expect(second.duplicateOfKey).toBeNull()
  })
})

describe(`toLinearBundle (project routing)`, () => {
  const bundle = toLinearBundle(linearSnapshotFixture(), { routing: `project` })

  it(`routes project issues to the project board, the rest to the team board, no project label`, () => {
    expect(bundle.boards.map((board) => board.key)).toEqual([
      `team:${T_MET}`,
      `team:${T_SOV}`,
      `project:${P_MAINT}`,
      `archive:${T_MET}`,
    ])
    const first = bundle.issues.find((issue) => issue.key === `issue:is-1`)!
    expect(first.boardKey).toBe(`project:${P_MAINT}`)
    expect(first.labelKeys).not.toContain(`project:${P_MAINT}`)
    const second = bundle.issues.find((issue) => issue.key === `issue:is-2`)!
    expect(second.boardKey).toBe(`team:${T_MET}`)
    // Only the archived issue keeps its project label: the archive board
    // says nothing about it.
    const fourth = bundle.issues.find((issue) => issue.key === `issue:is-4`)!
    expect(fourth.boardKey).toBe(`archive:${T_MET}`)
    expect(fourth.labelKeys).toContain(`project:${P_MAINT}`)
    expect(bundle.labels.filter((label) => label.key.startsWith(`project:`))).toHaveLength(1)
  })

  it(`has no project labels at all without archived issues in projects`, () => {
    const live = toLinearBundle(linearSnapshotFixture(), { routing: `project`, importArchived: false })
    expect(live.labels.some((label) => label.key.startsWith(`project:`))).toBe(false)
  })
})

describe(`linearPreview`, () => {
  const preview = linearPreview(linearSnapshotFixture())

  it(`counts per team, project, status, label and user`, () => {
    expect(preview.workspace).toEqual({ name: `Methode 5`, url: `https://linear.app/methode5` })
    expect(preview.teams.map((team) => [team.prefix, team.issueCount])).toEqual([
      [`MET`, 3],
      [`SOV`, 0],
    ])
    expect(preview.projects).toEqual([
      { key: `project:${P_MAINT}`, name: `M5 - Maintenance`, teamKey: `team:${T_MET}`, issueCount: 2 },
    ])
    expect(preview.supportsProjectRouting).toBe(true)
    expect(preview.labels.some((label) => label.key.startsWith(`project:`))).toBe(false)
    expect(preview.counts).toEqual({ issues: 4, comments: 3, assets: 2, assetBytes: 85_285, events: 5 })
    expect(preview.archives).toEqual([
      { key: `archive:${T_MET}`, teamKey: `team:${T_MET}`, name: `Methode 5 Archive`, prefix: `META`, issueCount: 1 },
    ])
    expect(preview.statuses.find((status) => status.key === `state:st-done`)).toMatchObject({
      teamKey: `team:${T_MET}`,
      issueCount: 2,
    })
  })

  it(`warns about what is dropped, and estimates and sub-issues are not`, () => {
    expect(preview.warnings.join(`\n`)).not.toMatch(/estimate/)
    expect(preview.warnings.join(`\n`)).not.toMatch(/sub-issue/)
    expect(preview.warnings.join(`\n`)).toMatch(/integration account/)
    const snapshot = linearSnapshotFixture()
    snapshot.issues[2]!.parentId = `is-gone`
    expect(linearPreview(snapshot).warnings.join(`\n`)).toMatch(/1 sub-issue\(s\) have a parent outside/)
  })
})
