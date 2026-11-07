// VAPP-272: `outbox.json` `syncedCases`, the crate's `tests/synced.rs` twin.
import { describe, expect, test } from "bun:test"

import outbox from "../fixtures/outbox.json" with { type: "json" }
import type { Pushed } from "./instance"
import type { OutboxEntry } from "./outbox"
import { SyncedRecordStore, type OutboxLog } from "./synced"

const memoryLog = (start: OutboxEntry[]) => {
  const rows = [...start]
  const log: OutboxLog = {
    entries: async () => [...rows],
    push: async (e) => void rows.push(e),
    ack: async (id, at) => {
      const i = rows.findIndex((e) => e.writeId === id)
      if (i >= 0) rows[i] = { ...rows[i]!, state: `acked`, ackedAt: at }
    },
    remove: async (ids) => {
      for (const id of ids) {
        const i = rows.findIndex((e) => e.writeId === id)
        if (i >= 0) rows.splice(i, 1)
      }
    },
  }
  return { log, rows }
}

type Reply = { ok?: boolean; transport?: boolean; code?: string; message?: string; invalid?: boolean; answer?: unknown }

describe(`the account's queue`, () => {
  for (const c of outbox.syncedCases) {
    test(c.name, async () => {
      const { log, rows } = memoryLog(c.log as OutboxEntry[])
      const replies = new Map(Object.entries(c.transport as Record<string, Reply[]>).map(([k, v]) => [k, [...v]]))
      const sent: string[] = []
      const refused: unknown[] = []
      const send = async (_app: string, _call: string, input: Record<string, unknown>): Promise<Pushed> => {
        const writeId = input.writeId as string
        sent.push(writeId)
        const reply = replies.get(writeId)?.shift() ?? { transport: true }
        const result = reply.ok ? { ok: true as const } : reply.code ? { code: reply.code } : { transport: true as const }
        return { result, answer: reply.answer, message: reply.message, invalid: reply.invalid } as Pushed
      }
      const store = await SyncedRecordStore.open(log, { push: async () => ({ transport: true }), send }, () => outbox.syncedNow, {
        refused: (entry, verb, pushed) => refused.push({ writeId: entry.writeId, verb, message: pushed.message ?? null }),
      })
      for (const step of c.steps as { do: string; entry?: OutboxEntry; online?: boolean; expected?: unknown; app?: string; rows?: never[] }[]) {
        if (step.do === `submit`) expect(await store.submit(step.entry!, step.online!)).toEqual(step.expected as never)
        else if (step.do === `flush`) await store.flush()
        else if (step.do === `rows`) await store.setRows(step.app!, step.rows!)
        else await store.prune()
      }
      expect(sent).toEqual(c.sent)
      expect(refused).toEqual(c.refused)
      expect(rows.map((e) => ({ writeId: e.writeId, state: e.state }))).toEqual(c.logAfter as never)
      expect(store.entries().map((e) => ({ writeId: e.writeId, state: e.state }))).toEqual(c.logAfter as never)
    })
  }
})
