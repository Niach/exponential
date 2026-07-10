import { describe, expect, it } from "vitest"
import {
  classifyWorkspacesForUserDeletion,
  type MembershipRow,
} from "./account-deletion"

// The orphan guard behind users.deleteAccount AND admin.deleteUser: deleting
// a user must never silently strand a multi-member workspace without an
// owner, and solo workspaces are deleted along with the account.

const USER = `user-1`

function m(
  workspaceId: string,
  userId: string,
  role: `owner` | `member`,
  isAgent = false
): MembershipRow {
  return { workspaceId, userId, role, isAgent }
}

describe(`classifyWorkspacesForUserDeletion`, () => {
  it(`flags a workspace as stranded when the user is the sole owner with other members`, () => {
    const rows = [m(`ws-a`, USER, `owner`), m(`ws-a`, `user-2`, `member`)]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-a`],
      solo: [],
    })
  })

  it(`flags a workspace as solo when the user is the entire membership`, () => {
    const rows = [m(`ws-personal`, USER, `owner`)]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [`ws-personal`],
    })
  })

  it(`treats a solo membership as solo even when the role is member (defensive)`, () => {
    const rows = [m(`ws-x`, USER, `member`)]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
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
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`allows deletion when the user is a plain member of someone else's workspace`, () => {
    // Even when that workspace has a sole owner — the OWNER is not the one
    // being deleted, so nothing is stranded.
    const rows = [m(`ws-a`, `user-2`, `owner`), m(`ws-a`, USER, `member`)]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`ignores workspaces the user is not a member of`, () => {
    const rows = [m(`ws-other`, `user-2`, `owner`)]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`classifies a mixed multi-workspace membership correctly`, () => {
    const rows = [
      // Personal workspace → solo.
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
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-team`],
      solo: [`ws-personal`],
    })
  })

  it(`returns nothing for an empty membership list`, () => {
    expect(classifyWorkspacesForUserDeletion([], USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`ignores the synthetic widget bot — a widget-owning personal workspace stays solo (REV-6)`, () => {
    // createWidgetUser adds an isAgent role=member row that widgets.delete
    // intentionally retains; it must never block account deletion.
    const rows = [
      m(`ws-p`, USER, `owner`),
      m(`ws-p`, `widget-bot`, `member`, true),
    ]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [`ws-p`],
    })
  })

  it(`still flags a workspace as stranded when a real member exists alongside a bot`, () => {
    const rows = [
      m(`ws-t`, USER, `owner`),
      m(`ws-t`, `widget-bot`, `member`, true),
      m(`ws-t`, `user-2`, `member`),
    ]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-t`],
      solo: [],
    })
  })

  it(`never counts an agent as another owner (defensive)`, () => {
    const rows = [
      m(`ws-t`, USER, `owner`),
      m(`ws-t`, `bot`, `owner`, true),
      m(`ws-t`, `user-2`, `member`),
    ]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-t`],
      solo: [],
    })
  })

  it(`counts the target's own rows even when the target is an agent (admin deleting a bot)`, () => {
    const rows = [m(`ws-x`, USER, `member`, true)]
    expect(classifyWorkspacesForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [`ws-x`],
    })
  })
})
