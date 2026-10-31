// Steer relay wire protocol (masterplan §3.2; steering v2 = EXP-249).
//
// VAPP-43: the relay carries NO session content. It is presence, control and
// eTalk calls:
//   - a CONTROL socket announces its device (`online`), receives `check_in`,
//     `kill {sessionId}`, `interrupt {sessionId}` and `compact_request` from
//     the web server's admin routes, and answers the last with
//     `compact_verdict`;
//   - a voice call's `caller` and `call_host` sockets (NEW-55) exchange the
//     call frames below.
// The session content frames (`CONTENT_FRAME_TYPES`: a session's activity,
// its history, and a viewer's steering) travel over eNet between a viewer
// and the device that runs the session (fixtures/steer-enet.json, which keeps
// the shapes in the "steer content frames" section below as their spec). The
// relay answers any of them, from any socket, `error content_refused` and
// drops it. Anything else that fails the parse is dropped too.
//
// NEW-55: BINARY frames exist for ONE pair of roles only — a voice call's
// `caller` and `call_host` sockets (fixtures/voice-call.json `wire`). Byte 0
// is the kind (`up` caller → host, `down` host → caller), bytes 1-2 a u16
// big-endian sequence, then one Opus packet; the relay forwards a frame whose
// kind matches the sender's role and whose length is within
// `CALL_MAX_FRAME_BYTES`, and drops everything else (a control socket's
// binary frame included). The call's JSON control frames (`call_mute`,
// `call_event`, `call_end`) ride beside them.
//
// The relay never decodes audio and never persists anything.

import { contract } from "@exp/domain-contract"
import scriptCard from "@exp/domain-contract/fixtures/script-card.json" with { type: "json" }
import voiceCall from "@exp/etalk/fixtures/voice-call.json" with { type: "json" }
import { z } from "zod"

// ── NEW-55: voice call wire constants (fixtures/voice-call.json) ─────────────

/** Byte 0 of a binary call frame: `up` = caller → host, `down` = host → caller. */
export const CALL_KIND_UP = voiceCall.wire.kinds.up
export const CALL_KIND_DOWN = voiceCall.wire.kinds.down
/** A whole binary frame (kind + seq + Opus packet) above this is dropped. */
export const CALL_MAX_FRAME_BYTES = voiceCall.wire.maxFrameBytes
/** kind (1) + u16 seq (2): a frame no longer than this carries no audio. */
export const CALL_FRAME_HEADER_BYTES = 3
/** How long a ringing call waits for the voice device's `call_host`. */
export const CALL_RING_TIMEOUT_MS = voiceCall.constants.ringTimeoutMs
/** NEW-55 review: per-socket binary frames per second (a token bucket). */
export const CALL_MAX_FRAMES_PER_SECOND = voiceCall.constants.maxFramesPerSecond
/** NEW-55 review: the longest call, and the silence that ends a paired one. */
export const CALL_MAX_MS = voiceCall.constants.maxCallMs
export const CALL_IDLE_TIMEOUT_MS = voiceCall.constants.idleTimeoutMs
/** One `call_event` payload, serialized, at most. A flow event is a line of
 *  transcript at worst; anything bigger is not one. */
export const CALL_EVENT_MAX_BYTES = 8 * 1024

// ── Client → relay control frames ────────────────────────────────────────────

export const onlineFrame = z.object({
  t: z.literal(`online`),
  deviceId: z.string().min(1).max(128),
  // EXP-672: presence is the deviceId → socket map and nothing more.
  // Non-strict parsing drops unknown keys; never `.strict()` — an extra key
  // would fail the parse and the device would silently go dark.
})

// ── Steer content frames (eNet only; the relay refuses them) ─────────────────
//
// VAPP-43: the shapes a viewer and the device running a session exchange
// over eNet's chat channel (fixtures/steer-enet.json). None of them is a
// relay client frame any more: see `CONTENT_FRAME_TYPES`.

export const helloFrame = z.object({
  t: z.literal(`hello`),
  sessionId: z.string().min(1).max(128),
  issueId: z.string().max(128).optional(),
  // EXP-90: publishers still send the removed public-activity feature's
  // `activityPublic: false` — non-strict parsing ignores it (do NOT add
  // `.strict()` here).
  // #EXP-1415: a device's one-shot journal REPLAY (`publish_history`), never
  // the run's live publisher. A replay never replaces a live publisher, nor
  // one inside its reconnect grace: the live one always wins.
  replay: z.boolean().optional(),
})

export const joinFrame = z.object({
  t: z.literal(`join`),
  // `activity` (the scrubbed member stream) is the ONLY audience, so the
  // literal is REQUIRED, not an optional field: parsing is non-strict, and a
  // dropped `channel` would silently admit any value a client sent.
  channel: z.literal(`activity`),
})

export const inputFrame = z.object({
  t: z.literal(`input`),
  // Keystrokes are tiny; anything big is not a keystroke.
  data: z.string().max(8 * 1024),
})

// Semantic answer to a `question` activity event (EXP-249): the client names
// the question it is answering, the publisher maps `keys` onto whatever the
// TUI currently shows. Gated exactly like `input` — any joined viewer
// (tickets are minted owner-only, EXP-312).
export const answerFrame = z.object({
  t: z.literal(`answer`),
  questionId: z.string().max(128),
  askId: z.string().max(128).optional(),
  keys: z.array(z.string().min(1).max(128)).min(1).max(10),
  // EXP-513: the typed reply for a `freeText` option — the desktop selects
  // the row with `keys`, types this into the TUI's inline editor and
  // submits. Bounded well under the input-frame cap; a reply is a line, not
  // a document.
  text: z.string().max(4000).optional(),
})

export const killFrame = z.object({ t: z.literal(`kill`) })
// EXP-790: the Stop button of a live session — interrupt the running turn
// (and drop what the agent holds queued behind it) WITHOUT ending the run.
// Gated exactly like `input`. Every client has sent it since EXP-790; the
// relay only started forwarding it with EXP-861.
export const interruptFrame = z.object({ t: z.literal(`interrupt`) })
// EXP-861: revoke ONE queued message by the id the `queue` slot named. Gated
// exactly like `input`; fire-and-forget — the next `queue` frame confirms.
export const unqueueFrame = z.object({
  t: z.literal(`unqueue`),
  id: z.string().min(1).max(128),
})
export const byeFrame = z.object({
  t: z.literal(`bye`),
  outcome: z.string().max(64).optional(),
})

// EXP-746 steering v2: the viewer changes ONE live agent option (model,
// effort, a thinking toggle) by naming the option id and the new value the
// publisher last advertised in `config_state`. FIRE-AND-FORGET: there is no
// ack frame — the publisher re-emits `config_state` once it applied, and that
// repaint IS the confirmation. Gated exactly like input/answer/kill.
export const setConfigFrame = z.object({
  t: z.literal(`set_config`),
  id: z.string().min(1).max(64),
  // NO `.min(1)`: the relay only forwards; the publisher validates the value
  // against the options it advertised (a blank one is its call to refuse).
  value: z.string().max(128),
})

// EXP-746: switch to one of the ids `config_state.modes[]` advertised.
export const setModeFrame = z.object({
  t: z.literal(`set_mode`),
  id: z.string().min(1).max(64),
})

// Publisher → relay: one activity event (the authenticated member activity
// channel). The desktop emits these from the Claude session transcript +
// worktree diffs, ALREADY REDACTED (known-secret masking + gitleaks-style
// patterns) — the relay stays a dumb pipe and fans them out to the activity
// audience, never interpreting a field.
//   narration:         assistant prose        { kind, text, beforeQuestionId?, messageId?, subagentId? }
//   thought:           a thought's edges      { kind, id, state, durationMs?, subagentId? }             (NEVER its text)
//   tool:              tool-call headline     { kind, name, detail?, subagentId? }
//   tool_update:       a settle / diff / output / MCP preview / script log, folded into its tool row
//   diff:              worktree unified diff  { kind, diff }  (latest replaces prior)
//   user_message:      a human turn           { kind, text, subagentId? }
//   question:          interactive question   { kind, text, options[], id, askId?, … }
//   question_resolved: retire a question card { kind, id?, askId?, answers?, dismissed? }
//   answer_ack:        injection confirmed    { kind, id, askId? }
//   subagent:          subagent lifecycle     { kind, id, agentType, status, title? }
//   permission:        informational prompt   { kind, tool, detail? }  (NOT answerable)
//   compaction:        context compaction     { kind, phase, trigger? }  (started|ended)
//   config_state:      live agent config      { kind, options[], currentMode?, modes[]?, commands[]? }  (latest replaces prior)
//   usage:             context + spend meter  { kind, contextUsed, contextSize, costUsd? }              (latest replaces prior)
//   context_layout:    where the window WENT   { kind, segments[{key,tokens,source,detail?}] }          (latest replaces prior)
//   rate_limit:        the agent's wall       { kind, status, resetsAt?, message? }                     (latest replaces prior)
//   api_error:         an agent API failure   { kind, message, errorType?, subagentId? }                (inline row, never a wall)
//   turn:              the turn edge          { kind, state, startedAt?, tokens? }                      (latest replaces prior)
//   background_tasks:  the bottom strip       { kind, tasks[] }                                         (latest replaces prior)
//   task_list:         the agent's own tasks  { kind, entries[{content,status}] }                       (latest replaces prior)
//   queue:             held user messages     { kind, messages[{id,text}] }                             (latest replaces prior)
//   workflow:          one Workflow card      { kind, id, name, status, phases[], agents[], … }         (latest replaces prior PER ID)
//
// EXP-850 / EXP-853 / EXP-856 pinned the last three plus `toolKind: "wait"`,
// `subagent.status: "duplicate"` and `subagent.workflowId` — the spec of record
// is `apps/steer-relay/STEER-WIRE-EXP-850.md`, and `crates/steer/src/frames.rs`
// mirrors every shape below byte for byte.
export const questionOptionSchema = z.object({
  label: z.string().max(256),
  // The `keys` member of the answer frame that picks this option — the
  // publisher turns it into whatever its TUI expects. EXP-746: on the ACP
  // path the key IS the agent's option id (`exit-plan-clear-accept-edits`,
  // `allow-once`), so the cap is the shared id cap, not a keystroke's.
  key: z.string().min(1).max(128),
  description: z.string().max(1024).optional(),
  // EXP-513: claude's synthetic free-text row ("Type something.") — clients
  // render an inline text input and send the typed reply as `answer.text`.
  // Must be declared here — the relay re-serializes the PARSED event, so an
  // undeclared field would be stripped.
  freeText: z.boolean().optional(),
})

/** EXP-783: the most events one `history_page` request may ask for, and one
 *  `history_chunk` may carry. Sized so a full page stays well inside the
 *  socket's 1 MiB payload limit and inside the per-viewer 512 KiB high-water
 *  mark the fan-out evicts over. The contract's `steerFeed.historyPageMax` —
 *  the same number every client asks with (EXP-795). */
export const HISTORY_PAGE_MAX = contract.steerFeed.historyPageMax

/** EXP-785: ACP's tool-call kinds, the contract's `toolKind.values`. */
export const TOOL_KINDS = contract.toolKind.values as [string, ...string[]]

/** EXP-786: what a `tool_update.diff` may weigh on the wire — the contract's
 *  `toolDiffMaxBytes` the publisher cuts to (on line boundaries), plus room
 *  for its one `\ N more lines truncated` marker line. Counted in UTF-16
 *  units by zod, which never exceeds the publisher's UTF-8 byte count. */
export const TOOL_DIFF_MAX_WIRE_BYTES = contract.steerFeed.toolDiffMaxBytes + 128

/** EXP-895: what a `tool_update.output` may weigh on the wire — the contract's
 *  `toolOutputMaxBytes` the publisher tail-cuts to (on line boundaries), plus
 *  room for its one `\ N more lines truncated` marker line. Counted in UTF-16
 *  units by zod, which never exceeds the publisher's UTF-8 byte count. */
export const TOOL_OUTPUT_MAX_WIRE_BYTES = contract.steerFeed.toolOutputMaxBytes + 128

/** EXP-846: what an Exponential MCP call settled on, as the tool's own answer
 *  named it — the issue it created, the PR it opened, how many rows a list
 *  returned. Published ONLY for `exponential_*` tools (nothing else has a
 *  result shape the engine knows) and every field optional: a tool that named
 *  none sends no preview at all. The result KIND is not on the wire — it is the
 *  contract's `expToolResults` entry for the tool's own name. */
/** EXP-920: one entity an Exponential tool's answer NAMED — what a viewer
 *  renders as a chip (issue, board, action, …) with a hover card resolved
 *  from its own synced rows. `kind` = contract `entityRefKind`; `id` = the
 *  row id (a `list` ref's id is its MEMBER kind); `count` only on a `list`.
 *  The engine distils these per contract `expToolPreview.tools` (the rule is
 *  fixture-locked ×2: web `lib/emcp/preview.ts`, engine `mapper.rs`). */
export const ENTITY_REF_KINDS = contract.entityRefKind.values as [
  string,
  ...string[],
]

/** The display cap on every preview / ref string, in UTF-16 code units (JS
 *  string length); publishers clamp to it. */
export const TOOL_PREVIEW_TEXT_MAX = contract.expToolPreview.textMax

/** A preview / ref string past the cap rejects the WHOLE frame: the
 *  publisher clamps first (UTF-16 units, never split surrogate pairs). */
const previewText = () => z.string().max(TOOL_PREVIEW_TEXT_MAX)

export const entityRefSchema = z.object({
  kind: z.enum(ENTITY_REF_KINDS),
  id: z.string().min(1).max(TOOL_PREVIEW_TEXT_MAX),
  identifier: previewText().optional(),
  title: previewText().optional(),
  count: z.number().int().nonnegative().max(4294967295).optional(),
})

export const toolPreviewSchema = z.object({
  id: previewText().optional(),
  identifier: previewText().optional(),
  title: previewText().optional(),
  url: previewText().optional(),
  count: z.number().int().nonnegative().max(4294967295).optional(),
  status: previewText().optional(),
  // EXP-920: the entities the answer named, chip by chip (≤ maxRefs).
  refs: z.array(entityRefSchema).max(contract.expToolPreview.maxRefs).optional(),
})

/** EXP-1279: a code-mode call's (`toolKind: script`) log on its settle —
 *  every call the script made (`count`), its wall time, the calls in the
 *  order it made them (at most the fixture's `logCap`; `tool` =
 *  `<server>.<tool>`, `subject` = what the call was about) and the script
 *  itself, cut like a command's output. Mirrored by `steer::frames::ScriptLog`,
 *  which clamps every field first. */
export const SCRIPT_LOG_MAX_CALLS = scriptCard.logCap
export const SCRIPT_TOOL_MAX = 128

export const scriptLogSchema = z.object({
  count: z.number().int().nonnegative().max(4294967295).optional(),
  elapsedMs: z.number().int().nonnegative().optional(),
  calls: z
    .array(
      z.object({
        tool: z.string().max(SCRIPT_TOOL_MAX),
        subject: previewText().optional(),
        ok: z.boolean(),
        ms: z.number().int().nonnegative().optional(),
      })
    )
    .max(SCRIPT_LOG_MAX_CALLS)
    .optional(),
  source: z.string().max(TOOL_OUTPUT_MAX_WIRE_BYTES).optional(),
})

/** EXP-850: every workflow / background-task string is cut to the contract's
 *  `steerWorking.previewMax` by the publisher, so the zod caps match it. */
export const WORKFLOW_TEXT_MAX = contract.steerWorking.previewMax

/** EXP-850 §2: contract `backgroundTaskKind` — claude's `task_type` folded
 *  onto four values. */
export const BACKGROUND_TASK_KINDS = contract.backgroundTaskKind.values as [
  string,
  ...string[],
]

/** EXP-927: contract `taskListStatus` — the agent's own task list. */
export const TASK_LIST_STATUSES = contract.taskListStatus.values as [
  string,
  ...string[],
]

/** EXP-850 §3: contract `workflowStatus` / `workflowAgentState`. */
export const WORKFLOW_STATUSES = contract.workflowStatus.values as [
  string,
  ...string[],
]
export const WORKFLOW_AGENT_STATES = contract.workflowAgentState.values as [
  string,
  ...string[],
]

/** EXP-856: contract `subagentStatus` — `duplicate` is the EXP-856 addition,
 *  a warning edge rather than a lifecycle one. */
export const SUBAGENT_STATUSES = contract.subagentStatus.values as [
  string,
  ...string[],
]

/** EXP-850 §2/§3: how many entries one frame may carry. Mirrored by
 *  `steer::BACKGROUND_TASKS_MAX` / `WORKFLOW_PHASES_MAX` /
 *  `WORKFLOW_AGENTS_MAX`, which the publisher truncates to first. */
export const BACKGROUND_TASKS_MAX = 32
// EXP-927: how many entries one `task_list` frame carries
// (`steer::TASK_LIST_MAX`, the publisher cuts the tail first).
export const TASK_LIST_MAX = 50
// EXP-861: how many user messages one run holds queued behind a running turn
// (the desktop engine's own cap), and how much of each the slot carries.
export const QUEUE_MAX = 20
export const QUEUE_TEXT_MAX = 8192
export const WORKFLOW_PHASES_MAX = 32
export const WORKFLOW_AGENTS_MAX = 64

/** EXP-1051: the attributed layers of a run's context window — the contract's
 *  `contextLayout` keys (a device naming anything else is publishing a layer
 *  no client can label), how it came by each number, and the cap on the
 *  human-readable `detail` behind one. A frame carries each key at most once
 *  and never more keys than the contract has. */
export const CONTEXT_SEGMENT_KEYS = contract.contextLayout.segments.map(
  (segment) => segment.key
) as [string, ...string[]]
export const CONTEXT_SEGMENT_SOURCES = contract.contextLayout.sources as [
  string,
  ...string[],
]
export const CONTEXT_DETAIL_MAX = contract.contextLayout.detailMax

export const activityEventSchema = z.discriminatedUnion(`kind`, [
  z.object({
    kind: z.literal(`narration`),
    text: z.string().max(16 * 1024),
    // EXP-483: claude withholds the transcript entry carrying an ask/plan
    // tool_use — prose included — until the picker resolves, so that prose
    // reaches the wire AFTER the already-published card. When set, this is
    // the claude tool_use_id of that ask/plan: clients splice the narration
    // immediately BEFORE the first question whose askId or id equals it
    // (no match → append). Must be declared here — the relay re-serializes
    // the PARSED event, so an undeclared field would be stripped.
    beforeQuestionId: z.string().max(128).optional(),
    // EXP-772: the agent's own message id. One message flushes in several
    // pieces on a long turn, so clients MERGE a narration row into the
    // previous one when both carry the same id.
    messageId: z.string().max(128).optional(),
    // EXP-773: set → the prose belongs INSIDE that subagent's card, never the
    // main feed (same identity as `tool.subagentId`).
    subagentId: z.string().max(128).optional(),
    at: z.number().optional(),
  }),
  // EXP-1324: a THOUGHT's lifecycle, never its text: `started` on the first
  // thought chunk, `ended` (with `durationMs`) when the thought closes (the
  // next non-thought update in its lane, or the turn end). Clients paint the
  // Trace Reasoning line off it; `id` pairs the two edges. A log row like
  // `tool` (replayed in order), subagent-scoped rows evict with that tier.
  z.object({
    kind: z.literal(`thought`),
    id: z.string().min(1).max(128),
    state: z.enum([`started`, `ended`]),
    durationMs: z.number().int().min(0).optional(),
    subagentId: z.string().max(128).optional(),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`tool`),
    name: z.string().max(128),
    detail: z.string().max(1024).optional(),
    // EXP-785: the ACP tool-call id — the key a later `tool_update` folds
    // into this row by — and ACP's kind bucket (the contract's `toolKind`).
    // Both REQUIRED: every publisher in the fleet emits them, so a tool row
    // always settles. The key is `toolKind`, never `kind`: `kind` is this
    // union's discriminator.
    id: z.string().min(1).max(128),
    toolKind: z.enum(TOOL_KINDS),
    // Set when the call came from a subagent's transcript (EXP-249) — clients
    // nest it under the matching `subagent` card.
    subagentId: z.string().max(128).optional(),
    at: z.number().optional(),
  }),
  // EXP-785/786/895: a tool call settled (`status`), an `edit` call's per-file
  // unified diff (`diff`, cut by the publisher to the contract's
  // toolDiffMaxLines/Bytes on line boundaries, plus one marker line) and an
  // `execute` call's `output` (the same, tail-cut — a command's verdict is its
  // last line — so its marker LEADS). A LOG row like `tool` — NOT latest-wins
  // — that clients fold INTO the tool row whose `id` matches, never a row of
  // its own; an unknown id is dropped.
  z.object({
    kind: z.literal(`tool_update`),
    id: z.string().min(1).max(128),
    status: z.enum([`completed`, `failed`]).optional(),
    diff: z.string().max(TOOL_DIFF_MAX_WIRE_BYTES).optional(),
    // EXP-895: sent ONCE, on the settle, and only for `execute` — the live
    // tail is the runner's own card, never a stream on the wire.
    output: z.string().max(TOOL_OUTPUT_MAX_WIRE_BYTES).optional(),
    at: z.number().optional(),
    // EXP-846: the subject an `exponential_*` call settled on.
    preview: toolPreviewSchema.optional(),
    // EXP-1279: a `script` call's log, on its settle only.
    script: scriptLogSchema.optional(),
  }),
  z.object({
    kind: z.literal(`diff`),
    diff: z.string().max(512 * 1024),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`user_message`),
    text: z.string().max(16 * 1024),
    // EXP-773: the subagent whose turn this is; set → the row renders inside
    // that subagent's card, never the main feed.
    subagentId: z.string().max(128).optional(),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`question`),
    // Larger than the narration budget — an ExitPlanMode plan rides here and
    // real plans clear 16KiB (EXP-691). Desktops truncate at 64KiB UTF-8
    // BYTES (>= UTF-16 code units, so this cap is satisfied for any script);
    // raise the two in lockstep only.
    text: z.string().max(64 * 1024),
    options: z.array(questionOptionSchema).min(1).max(10),
    multiSelect: z.boolean().optional(),
    // Marks an ExitPlanMode plan-approval picker (EXP-97) so clients can
    // render a dedicated "Plan ready" card. Presentation-only; absent on
    // AskUserQuestion events.
    planMode: z.boolean().optional(),
    // Stable identity (EXP-249), derived from the agent's tool call id
    // (claude `<tool_use_id>#<index>`, codex `<call_id>#<index>`, synthetic
    // for transcript-born cards). The `answer` frame names it, and a re-emit
    // with the same id REPLACES the card in place. Required (EXP-730).
    id: z.string().min(1).max(128),
    // Groups the steps of one multi-question ask. An askId question WITHOUT
    // index/total is that ask's final review/submit step.
    askId: z.string().max(128).optional(),
    index: z.number().int().min(1).optional(),
    total: z.number().int().min(1).optional(),
    header: z.string().max(256).optional(),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`question_resolved`),
    // Retire the card with this id; with only askId, retire every card of
    // that ask.
    id: z.string().max(128).optional(),
    askId: z.string().max(128).optional(),
    answers: z.array(z.string().max(1024)).max(10).optional(),
    dismissed: z.boolean().optional(),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`answer_ack`),
    // The desktop confirms it injected an answer — clients keep the card
    // locked instead of re-enabling it on a timeout.
    id: z.string().max(128),
    askId: z.string().max(128).optional(),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`subagent`),
    id: z.string().max(128),
    agentType: z.string().max(64),
    // EXP-856: `duplicate` joined the two lifecycle edges — a second copy of
    // an id that is still live (a `SendMessage` resumed a running agent). It
    // neither opens nor closes the run; the copy's own started/completed
    // still follow under the same id. Old clients ignore the unknown value.
    status: z.enum(SUBAGENT_STATUSES),
    detail: z.string().max(1024).optional(),
    at: z.number().optional(),
    // EXP-748: the publisher's count of this subagent's tool calls, stamped
    // on the completed edge. The replay log evicts subagent tool events
    // first, so this is what keeps "N tool calls" honest after eviction.
    // Bounded by u32: the desktop wire type is `Option<u32>`, and a larger
    // value makes `ViewerFrame::parse` drop the WHOLE activity frame.
    toolCalls: z.number().int().nonnegative().max(4294967295).optional(),
    // EXP-847: the spawning `Agent` call's own `description` — what the model
    // said this subagent is FOR. Clients show it on the chip and keep
    // `agentType` as a secondary caption; absent for codex, an external agent
    // and every pre-847 publisher.
    title: z.string().max(128).optional(),
    // EXP-850 §4: the `workflow.id` this agent belongs to. Clients nest such
    // an edge under the workflow card, never as a loose row, and the
    // `toolGroupSummary` / "N tool calls" counts never include it.
    workflowId: z.string().max(128).optional(),
  }),
  z.object({
    kind: z.literal(`permission`),
    tool: z.string().max(128),
    detail: z.string().max(1024).optional(),
    at: z.number().optional(),
  }),
  // EXP-724: the agent is compacting its context. `started` opens an
  // indeterminate "Compacting context…" strip on every viewer, `ended`
  // closes it (and leaves a "Context compacted" marker in the feed). A
  // publisher that can only observe the END (codex auto-compaction) sends a
  // bare `ended`; clients time a lone `started` out. `trigger` is claude's
  // PreCompact trigger; absent on codex.
  z.object({
    kind: z.literal(`compaction`),
    phase: z.enum([`started`, `ended`]),
    trigger: z.enum([`manual`, `auto`]).optional(),
    at: z.number().optional(),
  }),
  // EXP-746: the agent's LIVE configuration as the engine knows it — the chip
  // VOCABULARY and the values in force, re-emitted in full whenever anything
  // changes (a set_config/set_mode that landed, a model the agent switched
  // itself, an ACP current_mode_update / available_commands_update).
  // LATEST-WINS STATE, not a transcript row: the relay keeps only the newest
  // one per room and replays it after the log, so a joining viewer paints its
  // chips from one frame. Every field must be declared here — the relay
  // re-serializes the PARSED event, so an undeclared field is silently
  // stripped.
  z.object({
    kind: z.literal(`config_state`),
    options: z
      .array(
        z.object({
          id: z.string().min(1).max(64),
          label: z.string().max(128),
          // Grouping hint for the chip row (`model`, `effort`, …); a client
          // that doesn't know it renders one chip per option.
          category: z.string().max(32).optional(),
          // In force right now; blank = the CLI's own default.
          value: z.string().max(128).optional(),
          // Absent = read-only on this run (render the value, offer no menu).
          values: z
            .array(
              z.object({
                id: z.string().min(1).max(64),
                label: z.string().max(128),
              })
            )
            .max(32)
            .optional(),
        })
      )
      .max(8),
    currentMode: z.string().max(64).optional(),
    modes: z
      .array(
        z.object({
          id: z.string().min(1).max(64),
          label: z.string().max(128),
          description: z.string().max(256).optional(),
        })
      )
      .max(12)
      .optional(),
    // ACP available_commands_update: the AGENT's own slash commands. The `/`
    // menu shows the contract catalog UNION these (contract first).
    commands: z
      .array(
        z.object({
          name: z.string().min(1).max(64),
          description: z.string().max(256),
          hint: z.string().max(64).optional(),
        })
      )
      .max(64)
      .optional(),
    at: z.number().optional(),
  }),
  // EXP-746: the run's context window + spend as the engine last measured it.
  // LATEST-WINS state like `config_state` — never a feed row. Deliberately a
  // TOKEN count, not a percent: the device-reported DeviceUsageWindow already
  // owns the 0-100 rate-limit vocabulary and this is a different quantity.
  z.object({
    kind: z.literal(`usage`),
    contextUsed: z.number().int().min(0).max(1_000_000_000),
    contextSize: z.number().int().min(0).max(1_000_000_000),
    costUsd: z.number().min(0).max(1_000_000).optional(),
    at: z.number().optional(),
  }),
  // EXP-1051: where the window the `usage` meter measures actually WENT — the
  // layers the launcher put in before the first turn (`base`, `tools`,
  // `playbook`, `team`, `project`, `task`), each `measured` or `estimated`.
  // LATEST-WINS state beside `usage` (LATEST_WINS_KINDS/LATEST_REPLAY_ORDER in
  // hub.ts), never a feed row: the device publishes ONE per conversation and
  // again after a `/clear`, nothing on a compaction (the layers survive it).
  // A segment the device cannot attribute is simply omitted; `conversation`
  // and `free` are DERIVED by every client from `usage` and are not wire keys.
  z.object({
    kind: z.literal(`context_layout`),
    segments: z
      .array(
        z.object({
          key: z.enum(CONTEXT_SEGMENT_KEYS),
          // The same bounds as `usage`'s counts — a token is a token.
          tokens: z.number().int().min(0).max(1_000_000_000),
          source: z.enum(CONTEXT_SEGMENT_SOURCES),
          // What the layer is made of, as the device named it (`CLAUDE.md,
          // ~/.claude/CLAUDE.md`). The publisher cuts to the same cap.
          detail: z.string().max(CONTEXT_DETAIL_MAX).optional(),
        })
      )
      .max(CONTEXT_SEGMENT_KEYS.length),
    at: z.number().optional(),
  }),
  // EXP-784: the agent's rate-limit window as it last reported it. LATEST-WINS
  // state like `usage` (the fourth slot: LATEST_WINS_KINDS/LATEST_REPLAY_ORDER
  // in hub.ts). `status` is the agent's own word (`allowed_warning`,
  // `rejected`, …); an EMPTY status or `ok` CLEARS the slot on every client.
  // `resetsAt` is unix ms.
  z.object({
    kind: z.literal(`rate_limit`),
    status: z.string().max(64),
    resetsAt: z.number().int().min(0).optional(),
    message: z.string().max(1024).optional(),
    at: z.number().optional(),
  }),
  // An agent CLI's own transient API failure (claude's `API Error: …`: an
  // outage, a dropped connection, a 400): an INLINE transcript row like
  // `narration`, never a latest-wins slot and never a wall — only a
  // `rate_limit` walls a run. `errorType` is the CLI's classifier
  // (`server_error`, `invalid_request`, …). A relay older than this kind
  // rejects the frame — and a whole `history_chunk` page carrying one — so
  // the relay deploys BEFORE the desktop/CLI builds that publish it.
  z.object({
    kind: z.literal(`api_error`),
    message: z.string().max(1024),
    errorType: z.string().max(64).optional(),
    subagentId: z.string().max(128).optional(),
    at: z.number().optional(),
  }),
  // EXP-848: the END-OF-TURN signal — `started` while the agent is executing a
  // turn, `ended` the moment it is over (end_turn, a cancel, a failed prompt).
  // LATEST-WINS state (the fifth slot: LATEST_WINS_KINDS/LATEST_REPLAY_ORDER in
  // hub.ts), never a transcript row. Before the first one arrives every client
  // assumes `ended`, so nothing ever pulses by default.
  z.object({
    kind: z.literal(`turn`),
    state: z.enum([`started`, `ended`]),
    // EXP-850 §5: the turn's start (unix ms) on BOTH states, and the output
    // tokens it has produced so far — the working caption's inputs
    // (`{verb}… ({duration} · ↓ {tokens} tokens)`). The publisher republishes
    // the started slot with a growing count at most every
    // `steerWorking.tokenTickMs`, which is the ONE exception to the slot's
    // identical-frame dedupe; latest-wins, so it costs no replay growth.
    startedAt: z.number().int().min(0).optional(),
    tokens: z.number().int().min(0).max(1_000_000_000).optional(),
    // EXP-1501: the agent is STREAMING one tool call's input (claude
    // `input_json_delta`): `Writing {tool}… (54s)` on the working row
    // (`fixtures/working-caption.json`). Every frame carries the CURRENT
    // state, so a frame without it clears it.
    writing: z
      .object({
        tool: z.string().max(200),
        startedAt: z.number().int().min(0),
      })
      .optional(),
    at: z.number().optional(),
  }),
  // EXP-850 §2: the agent's background tasks, as the CLI last listed them in
  // FULL (an empty array = nothing running). LATEST-WINS state like `turn`,
  // never a transcript row: clients draw one line per task in the strip above
  // the composer, beside one `Waiting on {detail}` line per unsettled `wait`
  // tool row.
  z.object({
    kind: z.literal(`background_tasks`),
    tasks: z
      .array(
        z.object({
          id: z.string().min(1).max(128),
          kind: z.enum(BACKGROUND_TASK_KINDS),
          description: z.string().max(WORKFLOW_TEXT_MAX),
          // The launching tool_use_id, when the CLI named one.
          toolId: z.string().max(128).optional(),
          // EXP-1324: when the engine first saw the task (unix ms) — the
          // Queue's ticking elapsed time.
          startedAt: z.number().int().min(0).optional(),
        })
      )
      .max(BACKGROUND_TASKS_MAX),
    at: z.number().optional(),
  }),
  // EXP-927 (wire doc §2c): the agent's OWN task list (claude's TodoWrite /
  // Task* list, codex's plan), in FULL and in order. LATEST-WINS state like
  // `background_tasks`, never a transcript row: clients draw it as the first
  // block of the bottom strip, hidden while empty or all completed.
  z.object({
    kind: z.literal(`task_list`),
    entries: z
      .array(
        z.object({
          content: z.string().max(WORKFLOW_TEXT_MAX),
          status: z.enum(TASK_LIST_STATUSES),
        })
      )
      .max(TASK_LIST_MAX),
    at: z.number().optional(),
  }),
  // EXP-861: the user messages the agent has NOT read yet, in FULL and in
  // order (oldest first; an empty array = nothing queued, the bar closes).
  // LATEST-WINS state like `turn`, never a transcript row: each one arrives
  // as an ordinary `user_message` the moment the agent takes it in. EXP-873:
  // `sent` = already handed to the agent (a mid-turn message claude folds in
  // at its next tool boundary), awaiting its replay — no per-line revoke,
  // only Stop; absent = held on the device (a compaction is open), which the
  // `unqueue` frame revokes. The next `queue` frame is the confirmation.
  z.object({
    kind: z.literal(`queue`),
    messages: z
      .array(
        z.object({
          id: z.string().min(1).max(128),
          text: z.string().max(QUEUE_TEXT_MAX),
          sent: z.boolean().optional(),
        })
      )
      .max(QUEUE_MAX),
    at: z.number().optional(),
  }),
  // EXP-850 §3: one claude `Workflow` run's card. LATEST-WINS PER ID — the
  // relay keeps one frame per `workflow:{id}` key and replays them after the
  // log, before `background_tasks`. `id` is the `Workflow` tool call's own
  // id, so clients patch the card onto that tool row instead of appending a
  // second one. Every field must be declared here: the relay re-serializes
  // the PARSED event, so an undeclared one is silently stripped.
  z.object({
    kind: z.literal(`workflow`),
    id: z.string().min(1).max(128),
    name: z.string().max(WORKFLOW_TEXT_MAX),
    description: z.string().max(WORKFLOW_TEXT_MAX).optional(),
    status: z.enum(WORKFLOW_STATUSES),
    phases: z
      .array(
        z.object({
          index: z.number().int().min(0).max(4294967295),
          title: z.string().max(WORKFLOW_TEXT_MAX),
        })
      )
      .max(WORKFLOW_PHASES_MAX),
    agents: z
      .array(
        z.object({
          index: z.number().int().min(0).max(4294967295),
          label: z.string().max(WORKFLOW_TEXT_MAX),
          phaseIndex: z.number().int().min(0).max(4294967295).optional(),
          // The agent's own id — an EXP-856 duplicate `task_started` carries
          // exactly this string as its `task_id`.
          agentId: z.string().max(128).optional(),
          model: z.string().max(WORKFLOW_TEXT_MAX).optional(),
          state: z.enum(WORKFLOW_AGENT_STATES),
          tokens: z.number().int().min(0).optional(),
          toolCalls: z.number().int().min(0).max(4294967295).optional(),
          durationMs: z.number().int().min(0).optional(),
          lastTool: z.string().max(WORKFLOW_TEXT_MAX).optional(),
          lastToolSummary: z.string().max(WORKFLOW_TEXT_MAX).optional(),
          resultPreview: z.string().max(WORKFLOW_TEXT_MAX).optional(),
          error: z.string().max(WORKFLOW_TEXT_MAX).optional(),
        })
      )
      .max(WORKFLOW_AGENTS_MAX),
    summary: z.string().max(WORKFLOW_TEXT_MAX).optional(),
    at: z.number().optional(),
  }),
])

export type ActivityEvent = z.infer<typeof activityEventSchema>

// EXP-783: `seq` is the publisher's monotonic index for this event within the
// run — assigned by the device (seeded from its journal's line count, so a
// resumed run keeps counting) and ECHOED by the relay untouched. It is the
// only monotonic anchor on the wire, and it is what lets a client splice a
// join replay onto a transcript prefix it already holds instead of throwing
// everything away. REQUIRED: every publisher numbers its frames, so every
// row a client holds is addressable.
export const activityFrame = z.object({
  t: z.literal(`activity`),
  event: activityEventSchema,
  seq: z.number().int().min(0),
})

// EXP-783, viewer → relay: "send me the page of this run's transcript BELOW
// `beforeSeq`". Routed to the room's LIVE publisher, or (EXP-796) — in a
// history room the device already replayed and left, which LINGERS for
// `HISTORY_ROOM_LINGER_MS` — down the owning device's CONTROL socket, which
// answers with a `history_chunk` that names the session. The relay forwards
// it under an id of its own — viewers number their asks independently, so
// two in one room collide — and the answer is `history_chunk`s translated
// back to the viewer's `requestId`, delivered ONLY to the viewer that asked
// and never appended to the room's replay log.
export const historyPageFrame = z.object({
  t: z.literal(`history_page`),
  requestId: z.string().min(1).max(64),
  beforeSeq: z.number().int().min(0),
  limit: z.number().int().min(1).max(HISTORY_PAGE_MAX),
})

// EXP-783, publisher → relay: one page of older transcript, `requestId` being
// the RELAY's id from the `history_page` it answers. `done` marks the last
// chunk of a request (including a request with nothing to give). Never enters
// the room's log: these events are OLDER than everything the log holds, and
// the log is a join tail.
//
// EXP-796: `sessionId` is REQUIRED when the chunk comes down a device's
// CONTROL socket (one socket serves every session the machine ran, so the
// chunk has to say which room it answers); a publisher socket already
// belongs to its room and leaves it absent — its wire form is unchanged.
export const historyChunkFrame = z.object({
  t: z.literal(`history_chunk`),
  sessionId: z.string().min(1).max(128).optional(),
  requestId: z.string().min(1).max(64),
  events: z.array(activityEventSchema).max(HISTORY_PAGE_MAX),
  // One sequence per event, same order: the page is addressed by `seq`, so a
  // client can trim it against what it already holds.
  seqs: z.array(z.number().int().min(0)).max(HISTORY_PAGE_MAX),
  done: z.boolean(),
})

// #EXP-1415, control → relay: the answer to a `history_request` this device
// CANNOT serve (no journal for the session: pruned, or never written here).
// Without it the relay waited out its 20s timer while the viewer spun; now it
// answers at once and backs the session off, so redials stop asking.
export const historyNoneFrame = z.object({
  t: z.literal(`history_none`),
  sessionId: z.string().min(1).max(128),
})

// Publisher → relay: drop the replay log (EXP-249). The desktop sends this
// before re-publishing its full history on reconnect, so the log never
// doubles; the relay mirrors it to the audience as a "clear your feed" signal.
export const activityResetFrame = z.object({
  t: z.literal(`activity_reset`),
})

// ── Client → relay: the control socket's compaction verdict + eTalk calls ────

// EXP-936, control → relay: the host's answer to a `compact_request`. The
// DEVICE holds the facts the verdict needs (context occupancy, turns since
// the last compaction, whether a fold is already open), so it decides and
// the relay only carries the answer back to the web server's awaiting
// `POST /sessions/:id/compact` — the agent reads it off its tool result.
// One ask is in flight per session at a time, so the frame needs no request
// id: `sessionId` names the session the way `compact_request` did, and only
// the control socket of the device the ask went to may answer it.
// `refusedBecause` is one of the handler's four codes
// (`sessionsCompactRefusals`, lib/emcp/handlers/sessions-compact.ts); absent
// when accepted.
export const COMPACT_REFUSALS = [
  `too_early`,
  `cooldown`,
  `not_own_session`,
  `unsupported_agent`,
] as const
export type CompactRefusal = (typeof COMPACT_REFUSALS)[number]

export const compactVerdictFrame = z.object({
  t: z.literal(`compact_verdict`),
  sessionId: z.string().min(1).max(128),
  accepted: z.boolean(),
  refusedBecause: z.enum(COMPACT_REFUSALS).optional(),
})

// NEW-55, caller → relay → host: the caller's microphone closed or opened
// (replies keep playing either way). Forwarded verbatim.
export const callMuteFrame = z.object({
  t: z.literal(`call_mute`),
  muted: z.boolean(),
})

// NEW-55, host → relay → caller: ONE `flowCases` event (partial, said,
// queued, replyStarted, replyFinished, speechStarted) the caller folds into
// the same `callStep` machine. The relay checks only its shape (an object
// with a short `t`) and its size; the caller validates the rest.
export const callEventFrame = z.object({
  t: z.literal(`call_event`),
  event: z
    .looseObject({ t: z.string().min(1).max(32) })
    .refine(
      (event) =>
        Buffer.byteLength(JSON.stringify(event), `utf8`) <= CALL_EVENT_MAX_BYTES,
      { message: `call_event too large` }
    ),
})

// NEW-55, either side → relay → the other: hang up. The relay then closes
// both sockets and removes the room.
export const callEndFrame = z.object({
  t: z.literal(`call_end`),
  reason: z.string().max(64).optional(),
})

export const clientFrame = z.discriminatedUnion(`t`, [
  onlineFrame,
  compactVerdictFrame,
  callMuteFrame,
  callEventFrame,
  callEndFrame,
])

export type ClientFrame = z.infer<typeof clientFrame>

/** VAPP-43: every frame type of the old relay session protocol (a session's
 *  activity and history, a viewer's join and steering, a publisher's hello
 *  and bye) plus eNet's `leave`. The relay answers each, from any socket,
 *  `{t:"error", code:"content_refused"}` and drops it: session content never
 *  crosses the relay. */
export const CONTENT_FRAME_TYPES = [
  `hello`,
  `join`,
  `leave`,
  `input`,
  `answer`,
  `kill`,
  `interrupt`,
  `unqueue`,
  `set_config`,
  `set_mode`,
  `bye`,
  `activity`,
  `activity_reset`,
  `activity_synced`,
  `history_page`,
  `history_chunk`,
  `history_none`,
  `history_request`,
] as const
export type ContentFrameType = (typeof CONTENT_FRAME_TYPES)[number]

const CONTENT_FRAME_TYPE_SET: ReadonlySet<string> = new Set(CONTENT_FRAME_TYPES)

export function isContentFrameType(t: string): t is ContentFrameType {
  return CONTENT_FRAME_TYPE_SET.has(t)
}

// ── Relay → client control frames ────────────────────────────────────────────

export type ServerFrame =
  // Relay → a device's CONTROL socket, from the web server's admin routes
  // (`POST /sessions/:id/kill` and `/interrupt`): end the session, or
  // interrupt its running turn (EXP-1291; the session stays live).
  | { t: `kill`; sessionId: string }
  | { t: `interrupt`; sessionId: string }
  // EXP-988/EXP-936: relay → a device's CONTROL socket. The run's OWN MCP
  // call (`exponential_sessions_compact`) asked the host to compact the
  // agent's context; the host answers `compact_verdict` and does it at the
  // next turn boundary, or refuses. `keep` = what the summary must preserve.
  | { t: `compact_request`; sessionId: string; keep?: string }
  // EXP-481: fire-and-forget check-in nudge to a device's control socket —
  // the web server persisted new work (a queued command, edited launch
  // defaults) and an online device should heartbeat NOW instead of on its
  // next cadence. No reply frame exists; the heartbeat pickup is the durable
  // path.
  | { t: `check_in` }
  // NEW-55: relay → the voice device's CONTROL socket. A caller dialed it;
  // the device mints a `call_host` ticket for `callId` and joins within
  // CALL_RING_TIMEOUT_MS or the caller is told `no_answer`.
  | { t: `call_ring`; callId: string; agentId: string }
  // NEW-55: relay → caller AND host once the host joined: the pair is up.
  | { t: `call_ready` }
  // NEW-55: forwarded between the pair (see the client frames above).
  | { t: `call_mute`; muted: boolean }
  | { t: `call_event`; event: Record<string, unknown> }
  | { t: `call_end`; reason?: string }
  | { t: `bye`; outcome?: string }
  // VAPP-43: `content_refused` on any socket that sent a content frame (the
  // socket stays open). NEW-55: a CALLER may see `device_offline` (the voice
  // device's control socket is not connected), `busy` (a call already runs
  // on it) and `no_answer` (no host joined in time); a host `no_such_call`
  // (the room is gone, already answered, or not its own). Those four are
  // terminal: error, `bye` with the same outcome, close.
  | { t: `error`; code: string; message?: string }

// ── Close codes ───────────────────────────────────────────────────────────────

export const CLOSE_SESSION_ENDED = 4001
export const CLOSE_REPLACED = 4002
export const CLOSE_UNAUTHORIZED = 4003

export function parseClientFrame(raw: string): ClientFrame | null {
  let json: unknown
  try {
    json = JSON.parse(raw)
  } catch {
    return null
  }
  const parsed = clientFrame.safeParse(json)
  return parsed.success ? parsed.data : null
}

/** Why a frame failed [`parseClientFrame`], as LOG-SAFE fragments: the frame's
 * own `t` (sanitized, when it is a string at all) and the FIRST zod issue's
 * path + code. Never a value out of the frame — the activity channel is
 * scrubbed and `data`/`text`/`event` contents must never reach the relay log,
 * so only structural facts come back. Computed on the reject path only (it
 * re-parses), and rate-limited by the caller. */
export function describeClientFrameRejection(raw: string): {
  t: string | null
  issue: string
} {
  let json: unknown
  try {
    json = JSON.parse(raw)
  } catch {
    return { t: null, issue: `not_json` }
  }
  const rawT = (json as { t?: unknown } | null)?.t
  // A discriminator is a short keyword; anything else is hostile input, so it
  // is clipped and stripped of everything that could forge a log line.
  const t =
    typeof rawT === `string` ? rawT.slice(0, 32).replace(/[^\w-]/g, `?`) : null
  const parsed = clientFrame.safeParse(json)
  if (parsed.success) return { t, issue: `none` }
  const first = parsed.error.issues[0]
  const path = first?.path.join(`.`)
  return { t, issue: `${path || `<root>`}: ${first?.code ?? `invalid`}` }
}
