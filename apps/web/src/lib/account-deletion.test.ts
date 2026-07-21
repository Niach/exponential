import { describe, expect, it } from "vitest"
import {
  classifyTeamsForUserDeletion,
  type MembershipRow,
} from "./account-deletion"

// The orphan guard behind users.deleteAccount AND admin.deleteUser: deleting
// a user must never silently strand a multi-member team without an
// owner, and solo teams are deleted along with the account.

const USER = `user-1`

function m(
  teamId: string,
  userId: string,
  role: `owner` | `member`
): MembershipRow {
  return { teamId, userId, role }
}

describe(`classifyTeamsForUserDeletion`, () => {
  it(`flags a team as stranded when the user is the sole owner with other members`, () => {
    const rows = [m(`ws-a`, USER, `owner`), m(`ws-a`, `user-2`, `member`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-a`],
      solo: [],
    })
  })

  it(`flags a team as solo when the user is the entire membership`, () => {
    const rows = [m(`ws-personal`, USER, `owner`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [`ws-personal`],
    })
  })

  it(`treats a solo membership as solo even when the role is member (defensive)`, () => {
    const rows = [m(`ws-x`, USER, `member`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [`ws-x`],
    })
  })

  it(`allows deletion when another owner exists`, () => {
    const rows = [
      m(`ws-a`, USER, `owner`),
      m(`ws-a`, `user-2`, `owner`),
      m(`ws-a`, `user-3`, `member`),
    ]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`allows deletion when the user is a plain member of someone else's team`, () => {
    // Even when that team has a sole owner — the OWNER is not the one
    // being deleted, so nothing is stranded.
    const rows = [m(`ws-a`, `user-2`, `owner`), m(`ws-a`, USER, `member`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`ignores teams the user is not a member of`, () => {
    const rows = [m(`ws-other`, `user-2`, `owner`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`classifies a mixed multi-team membership correctly`, () => {
    const rows = [
      // Personal team → solo.
      m(`ws-personal`, USER, `owner`),
      // Sole owner of a team with members → stranded.
      m(`ws-team`, USER, `owner`),
      m(`ws-team`, `user-2`, `member`),
      // Co-owned team → fine.
      m(`ws-coowned`, USER, `owner`),
      m(`ws-coowned`, `user-3`, `owner`),
      // Plain membership elsewhere → fine.
      m(`ws-guest`, `user-4`, `owner`),
      m(`ws-guest`, USER, `member`),
    ]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-team`],
      solo: [`ws-personal`],
    })
  })

  it(`returns nothing for an empty membership list`, () => {
    expect(classifyTeamsForUserDeletion([], USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

})
