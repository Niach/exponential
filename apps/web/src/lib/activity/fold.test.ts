import { describe, it } from "vitest"

// EXP-988 contract table for `foldActivity` (lib/activity/fold.ts). Skipped
// until EXP-900 lands the implementation; that leaf un-skips every case and
// mirrors the table wherever the fold runs natively. Times are minutes from a
// common origin; every event sits on ONE issue unless stated.
describe.skip(`foldActivity (EXP-900)`, () => {
  it(`label added then removed by one actor within 1 min → gone`, () => {
    // [label_added L by A @0, label_removed L by A @1] → []
  })

  it(`status progress → in review by dev, reviewer moves back → BOTH kept`, () => {
    // [status A: backlog→in_progress @0, status B: in_progress→backlog @1]
    // → both rows unchanged (different actor).
  })

  it(`same actor, same field, 2 hours apart → both kept`, () => {
    // [status A: x→y @0, status A: y→x @120] → both rows unchanged.
  })

  it(`A → B → C by one actor within 2 min → one A → C event`, () => {
    // [status A: a→b @0, status A: b→c @2] → one status_changed row,
    // payload from = a, to = c, createdAt = @2, id = the last row's.
  })

  it(`A → B → A by one actor within the window → gone`, () => {})

  it(`a foreign event on the same issue in between breaks the run`, () => {
    // [status A: a→b @0, assignee B @1, status A: b→a @2] → all three kept.
  })

  it(`a foreign event on ANOTHER issue does not break the run`, () => {})

  it(`different fields by the same actor never fold into each other`, () => {
    // [status A @0, priority A @1] → both kept.
  })

  it(`creation events are never folded away`, () => {
    // [created A @0, status A: a→b @1, status A: b→a @2] → [created].
  })

  it(`pr_opened and pr_merged never fold`, () => {})

  it(`a run spanning more than the window from first to last is left alone`, () => {
    // [status A @0, status A @6, status A @11] → 11 min first-to-last → kept.
  })

  it(`returns the input untouched when nothing folds`, () => {})
})
