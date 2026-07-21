// Loads the canonical contract.json — the single source of truth for enum
// values used by the web, iOS, Android, and desktop clients. The generator
// script (`scripts/generate.ts`) emits per-language constants for the iOS,
// Android, and desktop clients; this file is the TypeScript entry point and
// is consumed by `@exp/db-schema/domain`.

import contractJson from "../contract.json" with { type: "json" }

export interface DomainContract {
  issueStatus: { values: readonly string[]; displayOrder: readonly string[] }
  issuePriority: { values: readonly string[]; displayOrder: readonly string[] }
  issueSource: { values: readonly string[] }
  teamRole: { values: readonly string[] }
  boardIcon: { values: readonly string[] }
  commentKind: { values: readonly string[] }
  notificationType: { values: readonly string[] }
  prState: { values: readonly string[] }
  codingSessionStatus: { values: readonly string[] }
  /**
   * Client-side liveness window for `running` coding_sessions rows: a row
   * whose synced updated_at is older than this renders as absent (EXP-153).
   * Mirrors CODING_SESSION_STALE_HOURS in @exp/db-schema/domain (the server
   * sweep's threshold) — parity locked by apps/web's domain-contract test.
   */
  codingSession: { staleHours: number }
  subscriberSource: { values: readonly string[] }
  issueEventType: { values: readonly string[] }
  /** Coding agent CLIs a desktop device may run (EXP-201; first = default). */
  codingAgent: { values: readonly string[] }
  /** Claude model aliases for coding-session launches (first = default). */
  codingModel: { values: readonly string[] }
  /** Claude effort levels; blank ("CLI default") is a per-client extra row, not a contract value. */
  codingEffort: { values: readonly string[] }
  /** Codex model slugs; blank ("CLI default") is a per-client extra row, not a contract value. */
  codexModel: { values: readonly string[] }
  /** Codex reasoning-effort levels (`model_reasoning_effort`); blank is per-client. */
  codexEffort: { values: readonly string[] }
  /** pi model patterns; blank ("CLI default") is a per-client extra row, not a contract value. */
  piModel: { values: readonly string[] }
  /** pi `--thinking` levels; blank is per-client. */
  piThinking: { values: readonly string[] }
}

export const contract = contractJson as unknown as DomainContract
