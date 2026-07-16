import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  issueStatusValues,
  issuePriorityValues,
  workspaceRoleValues,
  projectTypeValues,
  projectIconValues,
  commentKindValues,
  notificationTypeValues,
  prStateValues,
  codingSessionStatusValues,
  subscriberSourceValues,
  issueEventTypeValues,
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

  it(`workspace role values match the contract`, () => {
    expect([...workspaceRoleValues]).toEqual([...contract.workspaceRole.values])
  })

  it(`project type values match the contract`, () => {
    expect([...projectTypeValues]).toEqual([...contract.projectType.values])
  })

  it(`project icon values match the contract`, () => {
    expect([...projectIconValues]).toEqual([...contract.projectIcon.values])
  })

  it(`comment kind values match the contract`, () => {
    expect([...commentKindValues]).toEqual([...contract.commentKind.values])
  })

  it(`notification type values match the contract`, () => {
    expect([...notificationTypeValues]).toEqual([
      ...contract.notificationType.values,
    ])
  })

  it(`pr state values match the contract`, () => {
    expect([...prStateValues]).toEqual([...contract.prState.values])
  })

  it(`coding session status values match the contract`, () => {
    expect([...codingSessionStatusValues]).toEqual([
      ...contract.codingSessionStatus.values,
    ])
  })

  it(`subscriber source values match the contract`, () => {
    expect([...subscriberSourceValues]).toEqual([
      ...contract.subscriberSource.values,
    ])
  })

  it(`issue event type values match the contract`, () => {
    expect([...issueEventTypeValues]).toEqual([
      ...contract.issueEventType.values,
    ])
  })
})
