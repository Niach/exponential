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
  deviceIcon: { values: readonly string[] }
  commentKind: { values: readonly string[] }
  /** EXP-741: who posted a comment — a person, or an agent over MCP. */
  commentSource: { values: readonly string[] }
  notificationType: { values: readonly string[] }
  prState: { values: readonly string[] }
  /**
   * EXP-978: Workflows (DAG-orchestrated coding runs). The `wf` prefix keeps
   * them apart from `workflowStatus` below, the agent feed's word for a
   * Claude Code workflow TOOL run. Documented varchars on the rows.
   */
  wfStatus: { values: readonly string[] }
  wfNodeState: { values: readonly string[] }
  wfNodeKind: { values: readonly string[] }
  wfStartOn: { values: readonly string[] }
  wfRisk: { values: readonly string[] }
  wfReviewVerdict: { values: readonly string[] }
  /** EXP-1082: which workflow node a session belongs to and as what. */
  wfSessionRole: { values: readonly string[] }
  /** EXP-1082: the engine's event log (`workflow_events.kind`). */
  wfEventKind: { values: readonly string[] }
  /** EXP-1082: the FIVE node states a person sees (the stored
   *  `wfNodeState` is the engine's internal vocabulary). */
  wfNodeDisplayState: { values: readonly string[] }
  workflow: {
    maxParallelDefault: number
    maxIssues: number
    maxReviewRounds: number
    /** EXP-1082: `workflow_events` kept per workflow. */
    eventsMax: number
  }
  /** EXP-1029: the two-model workflow launch per agent (cheap `model`,
   *  capable `strongModel`) and a device's agent defaults. */
  workflowLaunch: {
    agents: readonly string[]
    claudeModel: string
    claudeStrongModel: string
    codexModel: string
    codexStrongModel: string
  }
  deviceAgentDefaults: {
    model: string
    subagentModel: string
    workflowModel: string
    workflowStrongModel: string
  }
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
   * EXP-1025: the team prompt's hard byte cap (`teams.agent_prompt`, UTF-8
   * bytes): zod on `teams.update`, the editors' live counter, and the
   * launcher's own guard before it appends the text to the system prompt.
   */
  team: { agentPromptMaxBytes: number }
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
    /**
     * EXP-895: the same cut, applied to a tool call's TEXT output before it
     * goes on the wire (a command's stdout, a read's contents) — the twin of
     * the `toolDiff*` caps, so a transcript row is a glance, never a dump.
     * The publisher truncates on LINE boundaries and appends the same
     * `\ N more lines truncated` marker the diff cut uses.
     */
    toolOutputMaxLines: number
    toolOutputMaxBytes: number
    /**
     * EXP-910: how many lines of a STILL-RUNNING call's output the live row
     * shows — a tail, the way a terminal shows a running command's last
     * words, so a chatty `bun test` cannot push the conversation off screen
     * while it runs. The settled row is unchanged: folded until the reader
     * opens it, and then the full `toolOutputMaxLines` cut.
     */
    liveToolOutputTailLines: number
  }
  /**
   * EXP-916: the diff UI's shared copy + numbers — session edit cards, the
   * Changes face and Reviews use these same words on every client. `{n}` /
   * `{hidden}` are substituted by the client. `cardPreviewFiles` = how many
   * rows an edited-files card lists before "N more"; `collapseThresholdLines`
   * = a file with more hunk lines starts collapsed; `lineChunk` = one "Show
   * more lines" step; `inlineDiffMaxHeight` = the transcript card's scroll
   * box height in px/pt.
   */
  diffUi: {
    filterPlaceholder: string
    changedFilesTitle: string
    editedFilesOne: string
    editedFilesMany: string
    moreFiles: string
    showLess: string
    showMoreLines: string
    mergePr: string
    closePr: string
    openOnGithub: string
    noChanges: string
    cardPreviewFiles: number
    collapseThresholdLines: number
    lineChunk: number
    inlineDiffMaxHeight: number
  }
  /**
   * EXP-1019: the launcher's shared copy — the ONE wording every composer
   * uses ×4 (web dialog + agent page, the IDE's start-coding dialog, the two
   * native composers). `runHeadline`/`implementHeadline` are the VERB in
   * front of the subject chips ("Run <action>", "Implement <issues>"); the
   * chips themselves are each client's own. `chatHeadline` is the subjectless
   * case, and the two placeholders are what the text field asks for once the
   * subject — not the prompt — is the main thing.
   */
  composerUi: {
    runHeadline: string
    implementHeadline: string
    chatHeadline: string
    chatPlaceholder: string
    instructionsPlaceholder: string
    dialogTitle: string
  }
  /**
   * EXP-785: ACP's tool-call kinds, carried on the `tool` steer event so
   * clients can bucket a call (an `edit` folds its diff, an `execute` is a
   * command) without parsing its name. Byte-equal to ACP's `ToolKind`.
   */
  toolKind: {
    values: readonly string[]
    /** EXP-916: the subset whose calls form an edited-files card (edit, delete, move). */
    editKinds: readonly string[]
  }
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
    /** EXP-981: the planner run of a draft workflow. */
    planWorkflowId: string
    /** EXP-984: the agent-review run of one workflow node (device-started). */
    reviewNodeId: string
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
   *
   * EXP-862: `title` + `blurb` are the same row's SETTINGS copy — the
   * built-in tools group of the MCP servers page lists one row per tool as
   * title + muted blurb, with the raw wire name only as a tooltip.
   */
  expToolDisplay: {
    prefix: string
    resultKinds: readonly string[]
    tools: readonly {
      name: string
      title: string
      blurb: string
      progressive: string
      done: string
      /** EXP-948: the captions a RUN of consecutive calls to this tool reads
       *  ("Reading {n} issues" / "Read {n} issues"), `{n}` = the member
       *  count. Our tools group by themselves and never hide inside a
       *  generic "N other tools" fold. */
      progressiveMany: string
      doneMany: string
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
  taskListStatus: { values: readonly string[] }
  /**
   * EXP-1051: the `context_layout` steer state — where a run's context window
   * goes, layer by layer. `segments` = the wire keys in RENDER order, each
   * with its legend label and bar tone (an Avatar hue, `neutral`, or `track`);
   * `derived` = the two rows every client computes from `usage` and never
   * receives (conversation, free); `sources` = measured|estimated;
   * `compactMinPercent` = the bar's first tick (`sessions_compact` refuses
   * below it); `charsPerToken` = the estimate every producer and editor
   * uses; `detailMax` = the relay's cap on a segment's `detail`.
   */
  contextLayout: {
    title: string
    segments: readonly { key: string; label: string; tone: string }[]
    derived: readonly { key: string; label: string; tone: string }[]
    sources: readonly string[]
    compactMinPercent: number
    charsPerToken: number
    detailMax: number
  }
  steerWorking: {
    verbs: readonly string[]
    tokenTickMs: number
    previewMax: number
  }
  /**
   * EXP-920: what an Exponential MCP tool's ANSWER can point at — the chip
   * kinds a run transcript renders for a settled `exponential_*` call. `list`
   * is the one-per-list-answer ref (its `id` = the member kind, its `count`
   * = how many rows the answer carried); everything else names ONE row.
   */
  entityRefKind: { values: readonly string[] }
  /**
   * EXP-920: the per-tool preview SPEC the engine distils refs by (the rule
   * home is web `lib/mcp/preview.ts`, mirrored by `crates/engine` and locked
   * ×2 by `fixtures/tool-result-preview.json`). Each tool's `refs` is a
   * whitespace-separated list of `kind@path` terms — `path` dotted into the
   * answer, `[]` fans an array out one ref per row, an empty path is the
   * answer itself; `list:<member>@path` yields the list ref plus up to
   * `maxRefs - 1` member refs. A tool with no row previews nothing.
   */
  expToolPreview: {
    maxRefs: number
    textMax: number
    tools: readonly { name: string; refs: string }[]
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

// EXP-895: the shared diff model + parser. Not part of contract.json (it is
// behaviour, not an enum table) but it lives here for the same reason
// `tool-group-summary` does: web, the styleguide and marketing all import it,
// and the natives mirror it against `fixtures/diff/*.json`.
export {
  parseDiff,
  parsePatch,
  pullFileStatus,
  fromPullFile,
  totals,
  mergeFilesByPath,
  unchangedBefore,
  unchangedBetween,
  unchangedLabel,
  additionsLabel,
  deletionsLabel,
  summaryLabel,
  renderDiff,
  DIFF_LINE_MAX,
  type Diff,
  type DiffFile,
  type DiffHunk,
  type DiffLine,
  type DiffStatus,
  type DiffTotals,
} from "./diff"

// EXP-916: the transcript's edited-files card and the Changes face's file
// tree — behaviour like the diff parser, mirrored ×4 and locked by
// `fixtures/feed/edit-cards.json` and `fixtures/diff/tree.json`.
export {
  editCard,
  editCardMoreLabel,
  editCardTitle,
  editRunEnd,
  isEditCall,
  renderEditCard,
  EDIT_CARD_KINDS,
  EDIT_CARD_PREVIEW,
  type EditCardFeedItem,
  type EditCardRow,
  type EditCardView,
  type EditRowState,
} from "./edit-card"

export {
  diffFileTree,
  renderDiffTree,
  type DiffTreeNode,
} from "./diff-tree"

// EXP-948: our OWN MCP calls never hide inside a generic collapsed run — the
// rule that keeps them visible and the caption a run of the SAME tool reads,
// locked by `fixtures/feed/exp-tool-groups.json`.
export {
  expToolGroupCaption,
  expToolIndex,
  expToolRowName,
  expToolRunEnd,
  isExpToolCall,
  EXP_TOOL_PREFIX,
  type ExpToolFeedItem,
} from "./exp-tool-group"
