// EXP-1216: the server reads a run's transcript — the one place the web app
// dials the steer relay as a VIEWER instead of calling its secret-authed
// HTTP admin routes. Transcripts never touch the database (they live on the
// run's device and stream through the relay, CLAUDE.md "Transcripts live
// ONLY on the device"), so `exponential_sessions_messages` joins the run's
// room exactly like a browser tab does (`steer-session-store.ts`): mint a
// viewer ticket, `join {channel:'activity'}`, collect the replayed
// `activity` frames until `activity_synced` names the span, then `bye`.
//
// The ticket is minted the way `steer.mintTicket` mints it (same seed, same
// helpers): `deviceId` + `deviceOwnerId` ride it ONLY for an ENDED run, so
// the relay asks that device to republish its journal instead of answering
// `no_such_session`; a live run's room is up while its publisher is.
//
// The socket is dialled at `STEER_RELAY_INTERNAL_URL` when set (the same
// server-to-server rule as `steerServerHttpBase`, EXP-504), else at the
// public `STEER_RELAY_URL`, translated to ws(s) by `steerTicketUrl`.

import {
  mintSteerTicket,
  steerTicketUrl,
  type SteerRelayConfig,
} from "@/lib/steer"
import type { SeqEvent } from "@/lib/steer-transcript-project"

/** Above the relay's own 20s HISTORY_TIMEOUT_MS, so an ended run's device
 *  gets the whole window to answer before we give up. */
export const TRANSCRIPT_READ_TIMEOUT_MS = 25_000

/** The slice of the WHATWG WebSocket the reader uses — Bun's global one in
 *  production, a fake in tests. */
export interface TranscriptSocket {
  onopen: ((event: unknown) => void) | null
  onmessage: ((event: { data: unknown }) => void) | null
  onerror: ((event: unknown) => void) | null
  onclose: ((event: unknown) => void) | null
  send(data: string): void
  close(code?: number, reason?: string): void
}
export type TranscriptSocketCtor = new (url: string) => TranscriptSocket

export interface RunTranscript {
  /** Every `activity` frame since the join's `activity_reset`, in arrival
   *  order (the projection sorts). */
  events: SeqEvent[]
  /** The span `activity_synced` named (0/0 for an empty room). */
  firstSeq: number
  lastSeq: number
  /** The relay holds only a tail: older events exist on the device only. */
  truncated: boolean
}

export type TranscriptErrorCode =
  | `no_such_session`
  | `device_offline`
  | `history_unavailable`
  | `timeout`
  | `closed`
  | `relay_error`

export class TranscriptReadError extends Error {
  readonly code: TranscriptErrorCode
  constructor(code: TranscriptErrorCode, message: string) {
    super(message)
    this.name = `TranscriptReadError`
    this.code = code
  }
}

function relayErrorMessage(code: string, timeoutMs: number): TranscriptReadError {
  switch (code) {
    case `no_such_session`:
      return new TranscriptReadError(
        code,
        `The run's device is not connected to the relay, so its transcript cannot be read now (the run may still be starting, or its device is offline). Retry later or use exponential_sessions_get.`
      )
    case `device_offline`:
      return new TranscriptReadError(
        code,
        `The device that ran this session is offline; its transcript lives only on that device. Retry once it is back online.`
      )
    case `history_unavailable`:
      return new TranscriptReadError(
        code,
        `The device that ran this session is online but returned no transcript (history unavailable: none stored, or pruned).`
      )
    case `timeout`:
      return new TranscriptReadError(
        code,
        `The relay did not finish sending the transcript within ${Math.round(timeoutMs / 1000)}s.`
      )
    default:
      return new TranscriptReadError(`relay_error`, `The relay refused the read (${code}).`)
  }
}

export interface ReadRunTranscriptInput {
  sessionId: string
  teamId: string
  /** The ticket's subject: the caller, already checked to be the run's
   *  owner or host (EXP-312 — the gate `steer.mintTicket` applies). */
  ownerUserId: string
  /** Set ONLY for an ended run: the machine holding its journal. */
  deviceId?: string | null
  /** The account that device is registered under (the host for a
   *  shared-device run, EXP-432). */
  deviceOwnerId?: string | null
  timeoutMs?: number
  /** Tests inject a fake; defaults to the runtime's global WebSocket. */
  WebSocket?: TranscriptSocketCtor
}

export function readRunTranscript(
  config: SteerRelayConfig,
  input: ReadRunTranscriptInput
): Promise<RunTranscript> {
  const timeoutMs = input.timeoutMs ?? TRANSCRIPT_READ_TIMEOUT_MS
  const minted = mintSteerTicket(config, {
    kind: `viewer`,
    userId: input.ownerUserId,
    teamId: input.teamId,
    sessionId: input.sessionId,
    ...(input.deviceId
      ? {
          deviceId: input.deviceId,
          deviceOwnerId: input.deviceOwnerId ?? input.ownerUserId,
        }
      : {}),
  })
  if (`disabled` in minted) {
    return Promise.reject(
      new TranscriptReadError(`relay_error`, `Remote steer is off on this instance.`)
    )
  }
  const url = steerTicketUrl(config.internalUrl ?? config.url, minted.ticket)
  const Ctor =
    input.WebSocket ??
    (globalThis as unknown as { WebSocket: TranscriptSocketCtor }).WebSocket

  return new Promise<RunTranscript>((resolve, reject) => {
    let events: SeqEvent[] = []
    let settled = false
    let relayError: TranscriptReadError | null = null
    const socket = new Ctor(url)

    const finish = (outcome: { ok: RunTranscript } | { error: Error }) => {
      if (settled) return
      settled = true
      clearTimeout(timer)
      socket.onopen = null
      socket.onmessage = null
      socket.onerror = null
      socket.onclose = null
      try {
        if (`ok` in outcome) socket.send(JSON.stringify({ t: `bye` }))
      } catch {
        // The socket may already be closing; the read is complete anyway.
      }
      try {
        socket.close(1000, `done`)
      } catch {
        // Ditto.
      }
      if (`ok` in outcome) resolve(outcome.ok)
      else reject(outcome.error)
    }

    const timer = setTimeout(
      () => finish({ error: relayErrorMessage(`timeout`, timeoutMs) }),
      timeoutMs
    )

    socket.onopen = () => {
      socket.send(JSON.stringify({ t: `join`, channel: `activity` }))
    }
    socket.onmessage = ({ data }) => {
      let frame: Record<string, unknown>
      try {
        frame = JSON.parse(typeof data === `string` ? data : String(data))
      } catch {
        return
      }
      switch (frame.t) {
        case `activity`: {
          const event = frame.event
          if (typeof frame.seq === `number` && event && typeof event === `object`) {
            events.push({
              seq: frame.seq,
              event: event as Record<string, unknown>,
            })
          }
          return
        }
        case `activity_reset`:
          // A reset means "drop what you rendered": a republish follows.
          events = []
          return
        case `activity_synced`:
          finish({
            ok: {
              events,
              firstSeq: typeof frame.firstSeq === `number` ? frame.firstSeq : 0,
              lastSeq: typeof frame.lastSeq === `number` ? frame.lastSeq : 0,
              truncated: frame.truncated === true,
            },
          })
          return
        case `error`:
          // Terminal codes are followed by a `bye` + close; remember the
          // code so whichever lands first rejects with it.
          relayError = relayErrorMessage(String(frame.code ?? ``), timeoutMs)
          finish({ error: relayError })
          return
        case `bye`:
          // The room closed before naming a span (the run ended mid-read):
          // what arrived is the transcript there is.
          if (events.length > 0) {
            const seqs = events.map((entry) => entry.seq)
            finish({
              ok: {
                events,
                firstSeq: Math.min(...seqs),
                lastSeq: Math.max(...seqs),
                truncated: false,
              },
            })
          } else {
            finish({
              error:
                relayError ??
                new TranscriptReadError(
                  `closed`,
                  `The relay closed the session (${String(frame.outcome ?? `ended`)}) before sending a transcript.`
                ),
            })
          }
          return
        default:
          // keepalive, history_pending, and anything newer: nothing to do.
          return
      }
    }
    socket.onerror = () => {
      finish({
        error:
          relayError ??
          new TranscriptReadError(`closed`, `Could not reach the steer relay.`),
      })
    }
    socket.onclose = () => {
      finish({
        error:
          relayError ??
          new TranscriptReadError(
            `closed`,
            `The steer relay closed the connection before the transcript was complete.`
          ),
      })
    }
  })
}
