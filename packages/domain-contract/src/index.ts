// Loads the canonical contract.json — the single source of truth for enum
// values used by the web, iOS, and Android clients. The generator script
// (`scripts/generate.ts`) emits per-language constants under the mobile
// apps; this file is the TypeScript entry point and is consumed by
// `@exp/db-schema/domain`.

import contractJson from "../contract.json" with { type: "json" }

export interface DomainContract {
  issueStatus: { values: readonly string[]; displayOrder: readonly string[] }
  issuePriority: { values: readonly string[]; displayOrder: readonly string[] }
  recurrenceUnit: { values: readonly string[] }
  workspaceRole: { values: readonly string[] }
  publicWritePolicy: { values: readonly string[] }
  commentKind: { values: readonly string[] }
  agentPlanState: { values: readonly string[] }
  notificationType: { values: readonly string[] }
  prState: { values: readonly string[] }
  runMode: { values: readonly string[] }
  subscriberSource: { values: readonly string[] }
  issueEventType: { values: readonly string[] }
  recurrenceIntervals: readonly number[]
}

export const contract = contractJson as unknown as DomainContract
