import { describe, expect, it } from "vitest"
import type { Board } from "@/db/schema"
import { compareBoards } from "@/hooks/use-team-data"

// EXP-525: the board order is a CROSS-CLIENT contract — the desktop IDE runs
// the same three keys in `crates/sync/src/collections.rs`. These cases are the
// ones where a "sort by sortOrder" implementation drifts.
function board(fields: Partial<Board>): Board {
  return {
    id: `00000000-0000-0000-0000-000000000000`,
    sortOrder: 0,
    createdAt: new Date(0),
    ...fields,
  } as Board
}

function order(boards: Board[]): string[] {
  return [...boards].sort(compareBoards).map((b) => b.id)
}

describe(`compareBoards`, () => {
  it(`sorts by sortOrder ascending`, () => {
    expect(
      order([
        board({ id: `c`, sortOrder: 2 }),
        board({ id: `a`, sortOrder: -1 }),
        board({ id: `b`, sortOrder: 1 }),
      ])
    ).toEqual([`a`, `b`, `c`])
  })

  it(`sorts a missing sortOrder LAST`, () => {
    expect(
      order([
        board({ id: `none`, sortOrder: null as never }),
        board({ id: `big`, sortOrder: 9999 }),
      ])
    ).toEqual([`big`, `none`])
  })

  it(`breaks a sortOrder tie by createdAt ascending`, () => {
    expect(
      order([
        board({ id: `newer`, createdAt: new Date(2_000) }),
        board({ id: `older`, createdAt: new Date(1_000) }),
      ])
    ).toEqual([`older`, `newer`])
  })

  it(`sorts a missing createdAt FIRST (Rust None < Some)`, () => {
    expect(
      order([
        board({ id: `dated`, createdAt: new Date(0) }),
        board({ id: `undated`, createdAt: null as never }),
      ])
    ).toEqual([`undated`, `dated`])
  })

  it(`breaks a full tie by id, byte-wise rather than by locale`, () => {
    // `localeCompare` folds case and ignores the hyphen, so it would call
    // these equal-ish and leave the order to the input; `<` must not.
    expect(
      order([
        board({ id: `b-1` }),
        board({ id: `B-1` }),
        board({ id: `a-1` }),
      ])
    ).toEqual([`B-1`, `a-1`, `b-1`])
  })

  it(`is a total order — every key participates`, () => {
    expect(
      order([
        board({ id: `d`, sortOrder: 1, createdAt: new Date(5) }),
        board({ id: `c`, sortOrder: 0, createdAt: new Date(9) }),
        board({ id: `b`, sortOrder: 0, createdAt: new Date(1) }),
        board({ id: `a`, sortOrder: 0, createdAt: new Date(1) }),
      ])
    ).toEqual([`a`, `b`, `c`, `d`])
  })
})
