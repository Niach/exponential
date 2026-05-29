import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  issueStatusValues,
  issuePriorityValues,
  recurrenceUnitValues,
  workspaceRoleValues,
  publicWritePolicyValues,
  commentKindValues,
  recurrenceIntervals,
  issueStatusOrder,
} from "@exp/db-schema/domain"

// Guards that the hand-maintained TS enums in @exp/db-schema/domain stay in
// lockstep with the canonical packages/domain-contract/contract.json. If they
// drift, this test fails — contract.json is the single source of truth, and the
// generated native constants (Swift/Kotlin) come from the same file.
describe(`domain-contract parity`, () => {
  it(`issue status values + display order match the contract`, () => {
    expect([...issueStatusValues]).toEqual([...contract.issueStatus.values])
    expect([...issueStatusOrder]).toEqual([...contract.issueStatus.displayOrder])
  })

  it(`issue priority values match the contract`, () => {
    expect([...issuePriorityValues]).toEqual([...contract.issuePriority.values])
  })

  it(`recurrence unit values match the contract`, () => {
    expect([...recurrenceUnitValues]).toEqual([
      ...contract.recurrenceUnit.values,
    ])
  })

  it(`workspace role values match the contract`, () => {
    expect([...workspaceRoleValues]).toEqual([...contract.workspaceRole.values])
  })

  it(`public write policy values match the contract`, () => {
    expect([...publicWritePolicyValues]).toEqual([
      ...contract.publicWritePolicy.values,
    ])
  })

  it(`comment kind values match the contract`, () => {
    expect([...commentKindValues]).toEqual([...contract.commentKind.values])
  })

  it(`recurrence intervals match the contract`, () => {
    expect([...recurrenceIntervals]).toEqual([...contract.recurrenceIntervals])
  })
})
