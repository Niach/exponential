// VAPP-33: the join replay, replayed from the fixture the device journal
// (apps/desktop/crates/steer/src/journal.rs) replays too — so the device's
// replay rules provably equal this relay's. The fixture's `$comment` is the
// spec.
import { describe, expect, test } from "bun:test"
import type { SteerTicketClaims } from "@exp/steer-ticket"
import fixture from "@exp/domain-contract/fixtures/steer-replay.json" with { type: "json" }
import { Hub, type RelaySocket } from "./hub"

type Json = null | boolean | number | string | Json[] | { [k: string]: Json }
type Event = { [k: string]: Json }
interface Generator {
  repeat: number
  event: Event
  padTo?: Record<string, number>
}
type Item = Event | Generator
interface Case {
  name: string
  startSeq?: number
  events: Item[]
  expect: {
    replay: (number | { from: number; to: number })[]
    kinds?: string[]
    firstSeq: number
    lastSeq: number
    truncated: boolean
  }
}

class FakeSocket implements RelaySocket {
  sent: string[] = []
  send(data: string) {
    this.sent.push(data)
  }
  sendBinary() {}
  close() {}
  bufferedAmount() {
    return 0
  }
  frames(): { t: string; [k: string]: unknown }[] {
    return this.sent.map((d) => JSON.parse(d))
  }
}

function claims(role: `publisher` | `viewer`): SteerTicketClaims {
  const now = Math.floor(Date.now() / 1000)
  return { sub: `user-1`, team: `team-1`, role, sessionId: `sess-1`, iat: now, exp: now + 60 }
}

function fill(value: Json, i: number): Json {
  if (typeof value === `string`) {
    return value.replace(/\{i(?:%(\d+))?\}/g, (_, mod?: string) =>
      String(mod ? i % Number(mod) : i)
    )
  }
  if (Array.isArray(value)) return value.map((v) => fill(v, i))
  if (value && typeof value === `object`) {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, fill(v, i)]))
  }
  return value
}

/** The fixture's event list, generators expanded. */
function expandEvents(items: Item[]): Event[] {
  const out: Event[] = []
  for (const item of items) {
    if (!(`repeat` in item) || typeof item.repeat !== `number`) {
      out.push(item as Event)
      continue
    }
    const gen = item as Generator
    for (let i = 0; i < gen.repeat; i++) {
      const event = fill(gen.event, i) as Event
      for (const [field, length] of Object.entries(gen.padTo ?? {})) {
        event[field] = String(event[field] ?? ``).padEnd(length, `x`)
      }
      out.push(event)
    }
  }
  return out
}

function expandSeqs(replay: Case[`expect`][`replay`]): number[] {
  return replay.flatMap((item) =>
    typeof item === `number`
      ? [item]
      : Array.from({ length: item.to - item.from + 1 }, (_, k) => item.from + k)
  )
}

describe(`steer-replay fixture (VAPP-33)`, () => {
  for (const c of (fixture as { cases: Case[] }).cases) {
    test(c.name, () => {
      const hub = new Hub()
      const pub = new FakeSocket()
      hub.onOpen(pub, claims(`publisher`))
      hub.onMessage(pub, JSON.stringify({ t: `hello`, sessionId: `sess-1`, issueId: `issue-1` }))
      let seq = c.startSeq ?? 0
      for (const event of expandEvents(c.events)) {
        hub.onMessage(pub, JSON.stringify({ t: `activity`, event, seq: seq++ }))
      }

      const viewer = new FakeSocket()
      hub.onOpen(viewer, claims(`viewer`))
      hub.onMessage(viewer, JSON.stringify({ t: `join`, channel: `activity` }))
      const frames = viewer.frames()
      expect(frames[0]?.t).toBe(`activity_reset`)
      const replay = frames.filter((f) => f.t === `activity`)
      expect(replay.map((f) => f.seq)).toEqual(expandSeqs(c.expect.replay))
      if (c.expect.kinds) {
        expect(
          replay.map((f) => {
            const event = f.event as { kind: string; id?: string }
            return event.kind === `workflow` ? `workflow:${event.id}` : event.kind
          })
        ).toEqual(c.expect.kinds)
      }
      const synced = frames.at(-1)!
      expect(synced.t).toBe(`activity_synced`)
      expect(synced.firstSeq).toBe(c.expect.firstSeq)
      expect(synced.lastSeq).toBe(c.expect.lastSeq)
      expect(synced.truncated === true).toBe(c.expect.truncated)
      hub.destroy()
    })
  }
})
