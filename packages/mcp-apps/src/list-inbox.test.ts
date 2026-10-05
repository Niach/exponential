import { inboxGroups, notificationConcept } from "./list-inbox"
import type { NotificationRow } from "./model"

const note = (id: string, issueId: string | null, readAt: string | null = null): NotificationRow => ({
  id,
  issueId,
  type: `issue_comment`,
  title: id,
  readAt,
  createdAt: `2026-10-05T10:00:00Z`,
})

describe(`notificationConcept`, () => {
  it(`maps types onto their concept, else the inbox bell`, () => {
    expect(notificationConcept(`pr_merged`)).toBe(`notification-pr-merged`)
    expect(notificationConcept(`session_blocked`)).toBe(`notification-session-blocked`)
    expect(notificationConcept(`brand_new`)).toBe(`nav-inbox`)
  })
})

describe(`inboxGroups`, () => {
  it(`folds an issue's notifications into its newest, messages stay single`, () => {
    const groups = inboxGroups([
      note(`n1`, `i1`),
      note(`m1`, null),
      note(`n2`, `i1`, `2026-10-05T11:00:00Z`),
      note(`m2`, null, `2026-10-05T11:00:00Z`),
    ])
    expect(groups.map((group) => [group.key, group.items.length, group.unread])).toEqual([
      [`issue:i1`, 2, 1],
      [`message:m1`, 1, 1],
      [`message:m2`, 1, 0],
    ])
  })
})
