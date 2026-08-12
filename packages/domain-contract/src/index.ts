// Loads the canonical contract.json — the single source of truth for enum
// values used by the web, iOS, Android, and desktop clients. The generator
// script (`scripts/generate.ts`) emits per-language constants for the iOS,
// Android, and desktop clients; this file is the TypeScript entry point and
// is consumed by `@exp/db-schema/domain`.

import contractJson from "../contract.json" with { type: "json" }

export interface DomainContract {
  issueStatus: { values: readonly string[]; displayOrder: readonly string[] }
  /**
   * Fixed status categories (EXP-314): every issue_statuses row belongs to
   * one. displayOrder is the ONE lifecycle order every surface speaks —
   * settings sections, set-status pickers and issue-list groups alike
   * (EXP-448 collapsed the separate settings order into it; it matches the
   * legacy issueStatus.displayOrder for a default team). startedMax caps how
   * many `started` statuses a team may have (the pie-clock fills are defined
   * only up to 4).
   */
  issueStatusCategory: {
    values: readonly string[]
    displayOrder: readonly string[]
    startedMax: number
  }
  /**
   * The 7 locked builtin statuses every team is seeded with (EXP-314) —
   * also the fallback set each client constructs locally when the
   * issue_statuses shape hasn't synced. Mirrored by the SQL seed in
   * apps/web/src/db/out/custom/0001_triggers.sql (parity-locked by the web
   * domain-contract test). Colors are seed DATA — builtin rows render via
   * each client's legacy token colors, not these hexes.
   */
  issueStatusDefaults: readonly {
    key: string
    category: string
    name: string
    color: string
    sortOrder: number
  }[]
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
  /**
   * EXP-481: how fresh a devices row's last_seen_at must be to render
   * "online" (devices heartbeat ~30s; the window is three missed beats).
   */
  device: { onlineWindowSeconds: number }
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
  /** Typed action-input kinds (EXP-257; EXP-259 adds `pr`): text | repo | board | pr. */
  actionInputType: { values: readonly string[] }
  /** Server-defined virtual actions injected into actions.list (EXP-257/EXP-259). */
  builtinAction: { createActionId: string; fixConflictsId: string }
  /** Action-input limits — parity-locked with @exp/db-schema/domain. */
  actionInputs: { max: number; maxTextLength: number }
}

export const contract = contractJson as unknown as DomainContract
