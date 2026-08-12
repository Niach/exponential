// Steer relay wire protocol (masterplan §3.2; steering v2 = EXP-249).
//
// TEXT frames only: JSON control messages `{ t, ... }`. The binary PTY mirror
// (opcode `0x01` + verbatim terminal bytes, the ring buffer, `resync`, viewer
// `resize` and the `pty` join channel) was REMOVED in EXP-249 — it had zero
// consumers, every client joins `channel: 'activity'`. Old desktops still push
// binary output frames and `resize`; both are accepted-then-ignored so their
// sessions keep working (`resize` stays IN the parser deliberately: a frame
// that parses and hits an explicit no-op case is quieter than one that fails
// the parse and takes the unknown-frame path).
//
// The relay is a dumb pipe with auth + ephemeral presence: it never parses
// terminal escape codes and never persists anything.

import { z } from "zod"

// ── Client → relay control frames ────────────────────────────────────────────

export const onlineFrame = z.object({
  t: z.literal(`online`),
  deviceId: z.string().min(1).max(128),
  deviceLabel: z.string().max(255).optional(),
  // EXP-201: the agent CLIs installed on the device (`claude`/`codex`/`pi`).
  // The relay is a dumb pipe — plain bounded strings here; the WEB SERVER
  // validates the vocabulary when a start names one. Absent (old desktop) ⇒
  // the hub defaults to ["claude"]. Since EXP-409 this list means RUNNABLE
  // (installed AND signed in); senders with signed-out agents send it
  // explicitly, even empty.
  agents: z.array(z.string().min(1).max(32)).max(16).optional(),
  // EXP-409: agents installed but SIGNED OUT — unusable (never in `agents`),
  // surfaced so machine lists can say "sign in on that machine". Same
  // dumb-pipe stance.
  unauthedAgents: z.array(z.string().min(1).max(32)).max(16).optional(),
  // EXP-253: feature capabilities (`actions`). Same dumb-pipe stance — the
  // web server interprets them. Absent (old desktop) ⇒ the hub defaults to
  // [] and action starts to that device are refused server-side.
  caps: z.array(z.string().min(1).max(32)).max(16).optional(),
  // EXP-437: the machine's per-agent launch defaults — remote Start-coding
  // dialogs seed from the selected device. Same dumb-pipe stance: bounded
  // strings only, vocabulary-free (clients validate against the contract).
  // Blank model/effort is meaningful ("CLI default"); booleans absent =
  // false. `.catch(undefined)` so a malformed/oversized blob degrades to
  // "no defaults" instead of failing the WHOLE online parse (a dropped
  // online frame reads as an offline machine).
  launchDefaults: z
    .object({
      defaultAgent: z.string().min(1).max(32).optional(),
      agents: z
        .record(
          z.string().min(1).max(32),
          z.object({
            model: z.string().max(64).optional(),
            effort: z.string().max(64).optional(),
            ultracode: z.boolean().optional(),
            planMode: z.boolean().optional(),
            skipPermissions: z.boolean().optional(),
          })
        )
        .refine((agents) => Object.keys(agents).length <= 16)
        .optional(),
    })
    .optional()
    .catch(undefined),
})

export type LaunchDefaults = NonNullable<z.infer<typeof onlineFrame>[`launchDefaults`]>

export const helloFrame = z.object({
  t: z.literal(`hello`),
  sessionId: z.string().min(1).max(128),
  issueId: z.string().max(128).optional(),
  // Geometry belonged to the removed PTY mirror — still accepted so old
  // desktops' hello frames parse, but the hub ignores it.
  cols: z.number().int().positive().max(1000).optional(),
  rows: z.number().int().positive().max(1000).optional(),
  // EXP-90: the removed public-activity feature's `activityPublic` flag may
  // still arrive from older desktops — non-strict parsing ignores it.
})

export const joinFrame = z.object({
  t: z.literal(`join`),
  // `activity` (the scrubbed member stream) is the ONLY audience. The legacy
  // value is still parsed so a `pty`/absent join can be answered with an
  // explicit `pty_removed` error instead of being silently dropped.
  channel: z.enum([`pty`, `activity`]).optional(),
})

// Legacy PTY geometry from old desktops — parsed, then ignored by the hub.
export const resizeFrame = z.object({
  t: z.literal(`resize`),
  cols: z.number().int().positive().max(1000),
  rows: z.number().int().positive().max(1000),
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
    // Question text shares the narration budget — an ExitPlanMode plan rides
    // here and can be large.
    text: z.string().max(16 * 1024),
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
  resizeFrame,
  inputFrame,
  answerFrame,
  killFrame,
  byeFrame,
  activityFrame,
  activityResetFrame,
])

export type ClientFrame = z.infer<typeof clientFrame>

// ── Relay → client control frames ────────────────────────────────────────────

/** Launch options a remote start may carry (EXP-149; agent/skipPermissions
 * are EXP-201). All optional — an absent field means "desktop settings
 * default" (plan mode OFF; absent agent = claude).
 * `startedBy` (EXP-432): the requesting teammate's userId on a start
 * targeting a SHARED server device — pure pass-through attribution the
 * daemon echoes into codingSessions.start; absent on own-device starts. */
export interface StartSessionOptions {
  startedBy?: string
  agent?: string
  model?: string
  effort?: string
  ultracode?: boolean
  planMode?: boolean
  skipPermissions?: boolean
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
  | { t: `input`; data: string } // viewer keystrokes, relay → publisher
  | { t: `answer`; questionId: string; askId?: string; keys: string[] } // relay → publisher
  | { t: `kill` }
  // EXP-481: fire-and-forget check-in nudge to a device's control socket —
  // the web server persisted new work (a queued command, edited launch
  // defaults) and an online device should heartbeat NOW instead of on its
  // next cadence. No reply frame exists; the heartbeat pickup is the durable
  // path. Only sent to devices whose registered caps prove a
  // check_in-aware build (the web server guards; old desktops never see it).
  | { t: `check_in` }
  | { t: `bye`; outcome?: string }
  | { t: `error`; code: string; message?: string }
  | { t: `activity`; event: ActivityEvent } // relay → activity audience (authenticated members only)
  | { t: `activity_reset` } // relay → activity audience: drop everything rendered so far

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
