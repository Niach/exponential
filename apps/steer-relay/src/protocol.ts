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
})

export const helloFrame = z.object({
  t: z.literal(`hello`),
  sessionId: z.string().min(1).max(128),
  issueId: z.string().max(128).optional(),
  cols: z.number().int().positive().max(1000).optional(),
  rows: z.number().int().positive().max(1000).optional(),
  // Whether the room's scrubbed activity stream may fan out to ANONYMOUS
  // public_viewer sockets (absent ⇒ true — legacy publishers stay public).
  // Authenticated activity-channel members receive activity regardless, so
  // a private session's own team can still follow along.
  activityPublic: z.boolean().optional(),
})

export const joinFrame = z.object({
  t: z.literal(`join`),
  // Which audience a VIEWER ticket joins: the PTY mirror (absent/`pty` —
  // legacy) or the scrubbed activity stream (`activity`). public_viewer
  // tickets ignore this — they are activity-only by construction.
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

// Publisher → relay: one PUBLIC activity event (feedback boards with
// publicShowCoding='live'). The desktop emits these from the Claude session
// transcript + worktree diffs, ALREADY REDACTED (known-secret masking +
// gitleaks-style patterns) — the relay stays a dumb pipe and fans them out to
// public_viewer sockets only, never to the PTY audience and never vice versa.
//   narration: assistant prose        { kind, text }
//   tool:      tool-call headline     { kind, name, detail? }
//   diff:      worktree unified diff  { kind, diff }  (latest replaces prior)
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

export type ServerFrame =
  | { t: `presence`; viewers: PresenceViewer[]; steererId: string | null }
  | { t: `resize`; cols: number; rows: number }
  | { t: `start_session`; issueId: string }
  | { t: `input`; data: string } // steerer keystrokes, relay → publisher
  | { t: `resync` }
  | { t: `kill` }
  | { t: `bye`; outcome?: string }
  | { t: `error`; code: string; message?: string }
  | { t: `activity`; event: ActivityEvent } // relay → activity audience (members always; public viewers only when the room is public)

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
