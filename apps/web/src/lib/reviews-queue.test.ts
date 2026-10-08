import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/reviews-queue.json"
import { reviewsNav, reviewsQueue } from "@/lib/reviews-queue"

describe(`reviewsNav (contract fixture)`, () => {
  for (const testCase of fixture.navCases) {
    it(testCase.name, () => {
      expect(reviewsNav(testCase.input)).toEqual(testCase.expected)
    })
  }
})

// EXP-1244: the Reviews queue, replayed from the contract fixture ×4 (desktop
// `reviews_queue_matches_the_fixture`, iOS `ReviewsQueueTests`, Android
// `ReviewsQueueTest`).
describe(`reviewsQueue (contract fixture)`, () => {
  for (const testCase of fixture.cases) {
    it(testCase.name, () => {
      const queue = reviewsQueue(testCase.input)
      expect({
        boardGroups: queue.boardGroups.map((group) => ({
          boardId: group.board.id,
          entries: group.entries.map((entry) => ({
            key: entry.key,
            issueIds: entry.issues.map((issue) => issue.id),
          })),
        })),
        runGroups: queue.runGroups.map((group) => ({
          teamId: group.teamId,
          sessionIds: group.sessions.map((session) => session.id),
        })),
        repoGroups: queue.repoGroups.map((repo) => ({
          teamId: repo.teamId,
          repositoryId: repo.repositoryId,
          pullNumbers: repo.pulls.map((pull) => pull.number),
        })),
        count: queue.count,
      }).toEqual(testCase.expected)
    })
  }
})
