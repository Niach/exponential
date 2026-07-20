// Steer relay wire protocol (masterplan §3.2).
//
// Two frame kinds on every socket:
//   - TEXT frames: JSON control messages `{ t, ... }` (this file).
//   - BINARY frames: terminal output — one opcode byte `0x01` followed by
//     verbatim PTY bytes (publisher → relay → viewers). Never JSON, never
//     base64 — this is the hot path.
//
// The relay is a dumb pipe with auth + ephemeral presence: it never parses
// terminal escape codes and never persists anything.

import { z } from "zod"

export const OUTPUT_OPCODE = 0x01

// ── Client → relay control frames ────────────────────────────────────────────

export const onlineFrame = z.object({
  t: z.literal(`online`),
  deviceId: z.string().min(1).max(128),
  deviceLabel: z.string().max(255).optional(),
  // EXP-201: the agent CLIs installed on the device (`claude`/`codex`/`pi`).
  // The relay is a dumb pipe — plain bounded strings here; the WEB SERVER
  // validates the vocabulary when a start names one. Absent (old desktop) ⇒
  // the hub defaults to ["claude"].
  agents: z.array(z.string().min(1).max(32)).max(16).optional(),
})

export const helloFrame = z.object({
  t: z.literal(`hello`),
  sessionId: z.string().min(1).max(128),
  issueId: z.string().max(128).optional(),
  cols: z.number().int().positive().max(1000).optional(),
  rows: z.number().int().positive().max(1000).optional(),
  // EXP-90: the removed public-activity feature's `activityPublic` flag may
  // still arrive from older desktops — non-strict parsing ignores it.
})

export const joinFrame = z.object({
  t: z.literal(`join`),
  // Which audience a VIEWER ticket joins: the PTY mirror (absent/`pty` —
  // legacy) or the scrubbed activity stream (`activity`).
  channel: z.enum([`pty`, `activity`]).optional(),
})

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

export const claimFrame = z.object({
  t: z.literal(`claim`),
  // steal:true (honored for perm `steer` only) overrides an existing steerer
  // — last-writer-wins. A plain claim stays first-claim-wins. Publisher
  // takeover still trumps everything.
  steal: z.boolean().optional(),
})
export const releaseFrame = z.object({ t: z.literal(`release`) })
export const killFrame = z.object({ t: z.literal(`kill`) })
export const byeFrame = z.object({
  t: z.literal(`bye`),
  outcome: z.string().max(64).optional(),
})

// Publisher → relay: one activity event (the authenticated member activity
// channel). The desktop emits these from the Claude session transcript +
// worktree diffs, ALREADY REDACTED (known-secret masking + gitleaks-style
// patterns) — the relay stays a dumb pipe and fans them out to the activity
// audience only, never to the PTY audience and never vice versa.
//   narration:    assistant prose        { kind, text }
//   tool:         tool-call headline     { kind, name, detail? }
//   diff:         worktree unified diff  { kind, diff }  (latest replaces prior)
//   user_message: a human turn           { kind, text }
//   question:     interactive question   { kind, text, options[] }
export const activityEventSchema = z.discriminatedUnion(`kind`, [
  z.object({
    kind: z.literal(`narration`),
    text: z.string().max(16 * 1024),
    at: z.number().optional(),
  }),
  z.object({
    kind: z.literal(`tool`),
    name: z.string().max(128),
    detail: z.string().max(1024).optional(),
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
    // `key` is the raw keystroke a steering client sends to pick the option.
    options: z
      .array(
        z.object({
          label: z.string().max(256),
          key: z.string().min(1).max(8),
        }),
      )
      .min(1)
      .max(10),
    multiSelect: z.boolean().optional(),
    // Marks an ExitPlanMode plan-approval picker (EXP-97) so clients can
    // render a dedicated "Plan ready" card. Presentation-only; absent on
    // AskUserQuestion events and on frames from older desktops.
    planMode: z.boolean().optional(),
    at: z.number().optional(),
  }),
])

export type ActivityEvent = z.infer<typeof activityEventSchema>

export const activityFrame = z.object({
  t: z.literal(`activity`),
  event: activityEventSchema,
})

export const clientFrame = z.discriminatedUnion(`t`, [
  onlineFrame,
  helloFrame,
  joinFrame,
  resizeFrame,
  inputFrame,
  claimFrame,
  releaseFrame,
  killFrame,
  byeFrame,
  activityFrame,
])

export type ClientFrame = z.infer<typeof clientFrame>

// ── Relay → client control frames ────────────────────────────────────────────

export interface PresenceViewer {
  userId: string
  name: string
  perm: `view` | `steer`
}

/** Launch options a remote start may carry (EXP-149; agent/skipPermissions
 * are EXP-201). All optional — an absent field means "desktop settings
 * default" (plan mode OFF; absent agent = claude). */
export interface StartSessionOptions {
  agent?: string
  model?: string
  effort?: string
  ultracode?: boolean
  planMode?: boolean
  skipPermissions?: boolean
}

/** Server-resolved repo group for a BATCH remote start — the desktop syncs no
 * repositories, so the frame carries everything the launcher needs to clone.
 * Never includes installationId (a server-only secret, stripped before it
 * reaches the relay). */
export interface StartRepoGroup {
  repositoryId: string
  fullName: string
  defaultBranch: string
}

export type ServerFrame =
  | { t: `presence`; viewers: PresenceViewer[]; steererId: string | null }
  | { t: `resize`; cols: number; rows: number }
  | ({ t: `start_session`; issueId: string } & StartSessionOptions)
  | ({
      t: `start_session`
      issueIds: string[]
      teamId: string
      repo: StartRepoGroup
    } & StartSessionOptions)
  | { t: `input`; data: string } // steerer keystrokes, relay → publisher
  | { t: `resync` }
  | { t: `kill` }
  | { t: `bye`; outcome?: string }
  | { t: `error`; code: string; message?: string }
  | { t: `activity`; event: ActivityEvent } // relay → activity audience (authenticated members only)

// ── Close codes ───────────────────────────────────────────────────────────────

export const CLOSE_SESSION_ENDED = 4001
export const CLOSE_REPLACED = 4002
export const CLOSE_UNAUTHORIZED = 4003
export const CLOSE_SLOW_CONSUMER = 4008

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
