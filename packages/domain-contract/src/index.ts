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
   * The 6 locked builtin statuses every team is seeded with (EXP-314;
   * Todo retired by EXP-685) —
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
  /** EXP-741: who posted a comment — a person, or an agent over MCP. */
  commentSource: { values: readonly string[] }
  notificationType: { values: readonly string[] }
  prState: { values: readonly string[] }
  codingSessionStatus: { values: readonly string[] }
  /**
   * EXP-637: how a run finished in the agent's own words
   * (`exponential_sessions_end`) and who ended it — parity-locked with
   * @exp/db-schema/domain by apps/web's domain-contract test.
   */
  codingSessionEndedBy: { values: readonly string[] }
  /**
   * EXP-804: the shape of `coding_sessions.blocked`, the agent's usage wall
   * as row state. NULL = not blocked; a blocked run still reads `running`
   * and stays live, steerable and killable, so this is orthogonal to status
   * exactly like `needs_input`. `kinds` is the discriminator (only the
   * rate-limit wall today), `windows` mirrors the agent usage windows a
   * device already reports (`session`/`weekly`/`model`).
   */
  codingSessionBlocked: {
    kinds: readonly string[]
    windows: readonly string[]
  }
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
  /**
   * EXP-783: the steering transcript's shared numbers, mirrored ×4 (web
   * `agent-feed.ts`, desktop `steer::feed`, iOS `AgentFeed`, Android
   * `AgentFeed.kt`) and by the relay's page schema. `byteCap`/`itemCap`
   * bound a client's copy of a run (sized to the device journal); `trim`
   * evicts oldest-first down to `trimTargetPercent` of a cap; each row
   * weighs its text plus `itemOverheadBytes`; `window`/`windowStep` are how
   * many of the newest rows a transcript renders and how many one "Load
   * earlier" adds; `historyPageMax` is the most events one `history_page`
   * may ask for. EXP-786: `toolDiffMaxLines`/`toolDiffMaxBytes` cap the
   * per-call unified diff a `tool_update` carries — the publisher truncates
   * on line boundaries (`steer::truncate_unified_diff`) before it rides the
   * wire, so every client sees the same patch.
   */
  steerFeed: {
    byteCap: number
    itemCap: number
    trimTargetPercent: number
    itemOverheadBytes: number
    window: number
    windowStep: number
    historyPageMax: number
    toolDiffMaxLines: number
    toolDiffMaxBytes: number
  }
  /**
   * EXP-785: ACP's tool-call kinds, carried on the `tool` steer event so
   * clients can bucket a call (an `edit` folds its diff, an `execute` is a
   * command) without parsing its name. Byte-equal to ACP's `ToolKind`.
   */
  toolKind: { values: readonly string[] }
  subscriberSource: { values: readonly string[] }
  /**
   * EXP-778: what a `pins` row points at — an issue, a coding session or an
   * action. One target column per kind; the sidebar's Pinned group renders
   * each kind with its own row.
   */
  pinKind: { values: readonly string[] }
  issueEventType: { values: readonly string[] }
  /**
   * EXP-736: issue relation types, with BOTH label halves in `values` order —
   * a row is stored in one canonical direction and each side renders its own
   * half (blocks/blocked by, parent of/sub-issue of, ...).
   */
  issueRelationType: {
    values: readonly string[]
    forwardLabels: readonly string[]
    inverseLabels: readonly string[]
  }
  /** Who created a relation row: an explicit pick, or a `#IDENT` reference. */
  issueRelationSource: { values: readonly string[] }
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
  /** Typed action-input kinds (EXP-257/EXP-259/EXP-273): repo | board | pr | icon — every one a PICK. EXP-825 retired the free-text kinds: a run's free text is the start's `prompt`. */
  actionInputType: { values: readonly string[] }
  /** EXP-792: how a team MCP server is reached — a remote `http` endpoint or a local `stdio` command. */
  mcpTransport: { values: readonly string[] }
  /** EXP-792: how the device authenticates to it — `none`, an `oauth` sign-in, or a typed `secret`. */
  mcpAuth: { values: readonly string[] }
  /** Server-defined virtual actions injected into actions.list (EXP-257/EXP-259). */
  builtinAction: {
    createActionId: string
    fixConflictsId: string
    chatId: string
  }
  /** Action-input limits — parity-locked with @exp/db-schema/domain. */
  actionInputs: { max: number; maxTextLength: number }
  /**
   * EXP-825: the free text a start carries beside its subject (the chat
   * prompt, or additional instructions for an issue/action run), in the
   * steer-image-message shape — prose plus up to `maxImages` trailing
   * `![image](/api/attachments/<id>)` embeds. `maxLength` caps the whole
   * string; parity-locked with @exp/db-schema/domain `MAX_START_PROMPT`.
   */
  startPrompt: { maxLength: number; maxImages: number }
  /**
   * Action automation triggers (EXP-530): the event kinds a trigger may
   * watch (a subset of issueEventType), the schedule intervals, and the cap
   * on each filter id list — parity-locked with @exp/db-schema/domain.
   */
  actionTrigger: {
    eventValues: readonly string[]
    scheduleIntervalValues: readonly string[]
    maxFilterIds: number
  }
  /**
   * EXP-530 automation-host tuning: cooldown after any triggered start, and
   * how far back an offline device replays issue_events on reconnect.
   */
  automation: { cooldownSeconds: number; eventCatchupHours: number }
  /**
   * EXP-724: the curated slash commands a steering client may offer in its
   * composer (`/` typeahead) and the desktop executes on the agent TUI.
   * Deliberately tiny: `/compact` and `/clear` are the two whose effect
   * every viewer can SEE (the compaction bar, a rotated conversation).
   * TUI-local printers (/cost, /status, /help), /model, /init, /review, the
   * login flow (EXP-430/444) and the kill path (/exit) stay out. A name is
   * the CATALOG's, not necessarily the CLI's: the desktop maps it per agent
   * (codex has no native `/clear`; the engine rotates the thread). `agents` ⊆
   * codingAgent.values; `argHint` empty = the command takes no argument;
   * `confirm` = the client asks before sending (context is discarded).
   * Generated into all four clients as parallel arrays; the desktop's
   * `steer::commands` catalog and every viewer's menu read the SAME rows.
   */
  steerCommands: {
    commands: readonly {
      name: string
      description: string
      argHint: string
      agents: readonly string[]
      confirm: boolean
    }[]
  }
  /**
   * EXP-846: the Exponential MCP tool display table, one row per tool the
   * server registers. A client strips `prefix` (after any `mcp__…__` MCP
   * namespace) off a tool call's name, finds the row and renders the brand
   * mark + `progressive` while the call runs, `done` once it settles, the
   * input field named by `subjectKey` as the subject (empty = none) and a
   * result preview keyed by `result` (one of `resultKinds`). Unknown tools
   * fall back to the raw name; generated ×4 as parallel arrays.
   */
  expToolDisplay: {
    prefix: string
    resultKinds: readonly string[]
    tools: readonly {
      name: string
      progressive: string
      done: string
      subjectKey: string
      result: string
    }[]
  }
  /**
   * EXP-848: the `turn` activity event's `state` — `started` when the agent
   * begins a turn, `ended` when it stops (end of turn, cancel, error). A
   * latest-wins slot on every client; the "Working…" predicate reads it.
   */
  turnState: { values: readonly string[] }
  // EXP-850/856: steer wire vocabulary — subagent edge statuses (incl. the
  // `duplicate` warning edge), workflow card states, background task kinds
  // and the claude-style working caption (verbs picked by turn start).
  subagentStatus: { values: readonly string[] }
  workflowAgentState: { values: readonly string[] }
  workflowStatus: { values: readonly string[] }
  backgroundTaskKind: { values: readonly string[] }
  steerWorking: {
    verbs: readonly string[]
    tokenTickMs: number
    previewMax: number
  }
}

export const contract = contractJson as unknown as DomainContract

export {
  toolGroupSummary,
  TOOL_GROUP_SUMMARY_SEPARATOR,
  type ToolCallSummary,
} from "./tool-group-summary"

export {
  workflowCaption,
  WORKFLOW_CAPTION_SEPARATOR,
  type WorkflowCaptionInput,
  type WorkflowCaptionAgent,
  type WorkflowCaptionPhase,
} from "./workflow-caption"
