import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  MODERATION_RESTRICTED_FIELDS,
  applyModerationRestrictions,
} from "@/lib/auth/access"

// The consolidated authorization layer (lib/auth/access.ts) sources its
// public-workspace moderation field list straight from the canonical
// contract.json, so the server clamp can never drift from the value generated
// into the native WorkspacePermissions constants. These tests lock that wiring
// and the strip behaviour (the only pure part of the predicate layer).
describe(`moderation clamp`, () => {
  it(`uses the canonical contract field list verbatim`, () => {
    expect([...MODERATION_RESTRICTED_FIELDS]).toEqual([
      ...contract.moderationRestrictedFields,
    ])
  })

  it(`strips exactly the restricted fields from an update payload`, () => {
    const updates: Record<string, unknown> = {
      title: `still allowed`,
      description: `still allowed`,
      labelIds: [`l1`],
      status: `in_progress`,
      priority: `high`,
      assigneeId: `u1`,
      dueDate: `2026-01-01`,
      dueTime: `09:00`,
      endTime: `10:00`,
      recurrenceInterval: 1,
      recurrenceUnit: `week`,
      archivedAt: new Date(),
    }

    applyModerationRestrictions(updates)

    // Open fields survive.
    expect(Object.keys(updates).sort()).toEqual(
      [`title`, `description`, `labelIds`].sort()
    )
    // Every contract-listed field is gone.
    for (const field of contract.moderationRestrictedFields) {
      expect(updates).not.toHaveProperty(field)
    }
  })

  it(`is a no-op when no restricted fields are present`, () => {
    const updates: Record<string, unknown> = {
      title: `t`,
      description: `d`,
    }
    applyModerationRestrictions(updates)
    expect(updates).toEqual({ title: `t`, description: `d` })
  })
})
