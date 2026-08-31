// Steer relay wire protocol (masterplan §3.2; steering v2 = EXP-249).
//
// TEXT frames only: JSON control messages `{ t, ... }`. A control socket
// announces device presence (`online`); a publisher opens its room (`hello`)
// and streams already-scrubbed `activity` events; a viewer `join`s the one
// audience there is (`channel: 'activity'`) and steers it with `input` /
// `answer` / `kill`. Anything else on the wire fails the parse and is
// dropped.
//
// The relay is a dumb pipe with auth + ephemeral presence: it never parses
// terminal escape codes and never persists anything.

import { z } from "zod"

// ── Client → relay control frames ────────────────────────────────────────────

export const onlineFrame = z.object({
  t: z.literal(`online`),
  deviceId: z.string().min(1).max(128),
  deviceLabel: z.string().max(255).optional(),
  // EXP-253: feature capabilities (`actions`). The relay is a dumb pipe, and
  // these ride along for presence listings only — the web server gates starts
  // on the persisted `devices` row.
  caps: z.array(z.string().min(1).max(32)).max(16).optional(),
})

export const helloFrame = z.object({
  t: z.literal(`hello`),
  sessionId: z.string().min(1).max(128),
  issueId: z.string().max(128).optional(),
  // EXP-90: publishers still send the removed public-activity feature's
  // `activityPublic: false` — non-strict parsing ignores it (do NOT add
  // `.strict()` here).
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

// Semantic answer to a `question` activity event (EXP-249). Replaces blind
// digit keystrokes: the client names the question it is answering, the
// publisher maps `keys` onto whatever the TUI currently shows. Gated exactly
// like `input` — any joined viewer (tickets are minted owner-only, EXP-312).
export const answerFrame = z.object({
  t: z.literal(`answer`),
  questionId: z.string().max(128),
  askId: z.string().max(128).optional(),
  keys: z.array(z.string().max(8)).min(1).max(10),
  // EXP-513: the typed reply for a `freeText` option — the desktop selects
  // the row with `keys`, types this into the TUI's inline editor and
  // submits. Bounded well under the input-frame cap; a reply is a line, not
  // a document.
  text: z.string().max(4000).optional(),
})

export const killFrame = z.object({ t: z.literal(`kill`) })
export const byeFrame = z.object({
  t: z.literal(`bye`),
  outcome: z.string().max(64).optional(),
})

// Publisher → relay: one activity event (the authenticated member activity
// channel). The desktop emits these from the Claude session transcript +
// worktree diffs, ALREADY REDACTED (known-secret masking + gitleaks-style
// patterns) — the relay stays a dumb pipe and fans them out to the activity
// audience, never interpreting a field.
//   narration:         assistant prose        { kind, text, beforeQuestionId? }
//   tool:              tool-call headline     { kind, name, detail?, subagentId? }
//   diff:              worktree unified diff  { kind, diff }  (latest replaces prior)
//   user_message:      a human turn           { kind, text }
//   question:          interactive question   { kind, text, options[], id?, askId?, … }
//   question_resolved: retire a question card { kind, id?, askId?, answers?, dismissed? }
//   answer_ack:        injection confirmed    { kind, id, askId? }
//   subagent:          subagent lifecycle     { kind, id, agentType, status }
//   permission:        informational prompt   { kind, tool, detail? }  (NOT answerable)
export const questionOptionSchema = z.object({
  label: z.string().max(256),
  // The raw keystroke a steering client sends to pick the option (also the
  // `keys` member of the semantic answer frame).
  key: z.string().min(1).max(8),
  description: z.string().max(1024).optional(),
  // EXP-513: claude's synthetic free-text row ("Type something.") — clients
  // render an inline text input and send the typed reply as `answer.text`.
  // Must be declared here — the relay re-serializes the PARSED event, so an
  // undeclared field would be stripped.
  freeText: z.boolean().optional(),
})

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
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`tool`),
    name: z.string().max(128),
    detail: z.string().max(1024).optional(),
    // Set when the call came from a subagent's transcript (EXP-249) — clients
    // nest it under the matching `subagent` card.
    subagentId: z.string().max(128).optional(),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`diff`),
    diff: z.string().max(512 * 1024),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`user_message`),
    text: z.string().max(16 * 1024),
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
    // AskUserQuestion events and on frames from older desktops.
    planMode: z.boolean().optional(),
    // Stable identity (EXP-249), derived from the claude tool_use_id. Present
    // ⇒ answerable via the semantic `answer` frame and re-emittable (same id
    // ⇒ clients REPLACE the card in place). Absent ⇒ old desktop, legacy
    // keystroke path.
    id: z.string().max(128).optional(),
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
    status: z.enum([`started`, `completed`]),
    detail: z.string().max(1024).optional(),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`permission`),
    tool: z.string().max(128),
    detail: z.string().max(1024).optional(),
    at: z.number().optional(),
  }),
])

export type ActivityEvent = z.infer<typeof activityEventSchema>

export const activityFrame = z.object({
  t: z.literal(`activity`),
  event: activityEventSchema,
})

// Publisher → relay: drop the replay log (EXP-249). The desktop sends this
// before re-publishing its full history on reconnect, so the log never
// doubles; the relay mirrors it to the audience as a "clear your feed" signal.
export const activityResetFrame = z.object({
  t: z.literal(`activity_reset`),
})

export const clientFrame = z.discriminatedUnion(`t`, [
  onlineFrame,
  helloFrame,
  joinFrame,
  inputFrame,
  answerFrame,
  killFrame,
  byeFrame,
  activityFrame,
  activityResetFrame,
])

export type ClientFrame = z.infer<typeof clientFrame>

// ── Relay → client control frames ────────────────────────────────────────────

/** Launch options a remote start may carry (EXP-149; `agent` is EXP-201).
 * All optional — an absent field means "desktop settings default" (plan mode
 * OFF; absent agent = claude). EXP-690 retired `skipPermissions`: every
 * launch bypasses the agent's permission prompts, and an old caller's key is
 * dropped here instead of forwarded.
 * `startedBy` (EXP-432): the requesting teammate's userId on a start
 * targeting a SHARED server device — pure pass-through attribution the
 * daemon echoes into codingSessions.start; absent on own-device starts. */
export interface StartSessionOptions {
  startedBy?: string
  /** EXP-679: the run was started by another coding session (through
   * `exponential_sessions_start`) — the device writes it as
   * `coding_sessions.started_reason` so the run is unattended, i.e. its own
   * `exponential_sessions_end` close-out ends it. Absent on human starts;
   * `agent` is the only value the relay accepts. */
  startedReason?: `agent`
  agent?: string
  model?: string
  effort?: string
  ultracode?: boolean
  planMode?: boolean
  /** EXP-481: resume the issue's existing worktree/agent session instead of
   * starting fresh. Single-issue starts only; the web server gates it on the
   * device's `resume` cap — the relay passes it through untouched. */
  resume?: boolean
}

/** Server-resolved repo group for a BATCH or ACTION remote start — the
 * desktop syncs no repositories, so the frame carries everything the
 * launcher needs to clone.
 * Never includes installationId (a server-only secret, stripped before it
 * reaches the relay). */
export interface StartRepoGroup {
  repositoryId: string
  fullName: string
  defaultBranch: string
}

/** EXP-257: one filled action input, fully resolved server-side (display =
 * repo fullName / board name / the text itself) so the desktop injects a
 * readable "## Inputs" block with no lookups. Dumb-pipe strings — the relay
 * never interprets them. */
export interface StartInput {
  key: string
  label: string
  type: string
  value: string
  display: string
}

export type ServerFrame =
  | ({ t: `start_session`; issueId: string } & StartSessionOptions)
  | ({
      t: `start_session`
      issueIds: string[]
      teamId: string
      repo: StartRepoGroup
    } & StartSessionOptions)
  // EXP-253 action run: actionName is a display snapshot (tab/trust-dialog
  // title before the desktop's own actions.get resolves); repo is absent for
  // repo-less actions. Since EXP-257 the full option set applies and
  // `inputs` carries the resolved input values.
  | ({
      t: `start_session`
      actionId: string
      actionName: string
      teamId: string
      repo?: StartRepoGroup
      inputs?: StartInput[]
    } & StartSessionOptions)
  // EXP-637: resume an ENDED run. The device's own run registry holds the
  // cwd, agent, options and native transcript id, so the frame only names
  // the run — the optional ids/branch are display hints the desktop can use
  // before its registry lookup resolves. No launch options ride a resume: a
  // resumed run keeps the ones it was started with.
  | {
      t: `start_session`
      resumeSessionId: string
      teamId: string
      issueId?: string
      actionId?: string
      actionName?: string
      branch?: string
      startedBy?: string
      /** EXP-679: the resume was asked for by another coding session — the
       * device writes it as `coding_sessions.started_reason` so the relaunched
       * run is unattended. */
      startedReason?: `agent`
    }
  | { t: `input`; data: string } // viewer keystrokes, relay → publisher
  | { t: `answer`; questionId: string; askId?: string; keys: string[]; text?: string } // relay → publisher
  | { t: `kill` }
  // EXP-481: fire-and-forget check-in nudge to a device's control socket —
  // the web server persisted new work (a queued command, edited launch
  // defaults) and an online device should heartbeat NOW instead of on its
  // next cadence. No reply frame exists; the heartbeat pickup is the durable
  // path.
  | { t: `check_in` }
  | { t: `bye`; outcome?: string }
  | { t: `error`; code: string; message?: string }
  | { t: `activity`; event: ActivityEvent } // relay → activity audience (authenticated members only)
  | { t: `activity_reset` } // relay → activity audience: drop everything rendered so far
  // EXP-648: relay → activity audience every VIEWER_KEEPALIVE_INTERVAL_MS so
  // a viewer can tell a quiet socket from a dead one (an agent parked on a
  // question or plan approval sends nothing for minutes, and the desktop's
  // 30s ping is a WS control frame that never reaches viewers). Carries
  // nothing, never changes a viewer's phase, and is only ever sent to a
  // socket whose join has already been answered.
  | { t: `keepalive` }
  // EXP-656: relay → the JOINING viewer only, right after `activity_reset` +
  // the replay — "the picture is complete, commit it". Lets a client stage a
  // reconnect's replay behind its visible feed and swap atomically, so a
  // reader scrolled up in a plan is never yanked to the bottom. Carries
  // nothing; every client ignores unknown `t` values, and a publisher-driven
  // reset + republish never gets one (old desktops give the relay no
  // end-of-republish signal — clients fall back to a quiet timer).
  | { t: `activity_synced` }

// ── Close codes ───────────────────────────────────────────────────────────────

export const CLOSE_SESSION_ENDED = 4001
export const CLOSE_REPLACED = 4002
export const CLOSE_UNAUTHORIZED = 4003
export const CLOSE_SLOW_CONSUMER = 4008
// EXP-283: idle-publisher eviction. Deliberately NOT 4001 — desktops treated
// CLOSE_SESSION_ENDED as a terminal remote kill (tear down the live agent +
// terminal tab), so an idle close after a laptop sleep >90s was killing live
// sessions on wake. Publishers treat this (and any unknown code) as a plain
// drop and reconnect; a truly dead publisher just never re-hellos and the
// room dies via the grace timer.
export const CLOSE_PUBLISHER_IDLE = 4009

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
