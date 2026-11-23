// #EXP-2067: chat-express.json replayed over the eui SDK's wasm (the same
// a2ui-express rev eui-core pins).
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { ExpressDecoder } from "../../exponential-ui/src/host/decoders"
import { initExpress, unwrapExpress } from "../../exponential-ui/src/express/index"
import fixture from "../fixtures/chat-express.json" with { type: "json" }
import {
  actionInput,
  expressProse,
  expressRow,
  ExpressRow,
  failureNote,
  parseActionInput,
  plainProse,
  surfaceAction,
  type ActionCard,
  type ExpressSdk,
  type RowSegment,
} from "./chat-express"
import { chatPreview } from "./chats"

initExpress(readFileSync(new URL(`../../exponential-ui/wasm/express/a2ui_express_bg.wasm`, import.meta.url)))

const sdk: ExpressSdk = {
  unwrap: (text) => unwrapExpress(text),
  decoder: (surfaceId) => new ExpressDecoder({ surfaceId, wrapped: false }),
}

/** A row as the fixture writes it (no messages, no generation, no SDK words). */
function locked(segments: RowSegment[] | null): unknown {
  if (segments === null) return null
  return segments.map((s) => {
    if (s.kind === `prose`) return s
    const { messages: _m, generation: _g, errorMessage: _e, ...rest } = s
    return rest
  })
}

describe(`segmentCases`, () => {
  for (const c of fixture.segmentCases) {
    test(c.name, () => {
      const row = expressRow(c.text, c.rowKey, c.final, sdk)
      expect(locked(row)).toEqual(c.expected as unknown)
      expect(expressProse(row, c.text)).toBe(c.prose)
    })
  }
})

describe(`streamCases`, () => {
  test(`one row, fragment by fragment`, () => {
    const row = new ExpressRow(fixture.streamCases[0]!.rowKey, sdk)
    for (const step of fixture.streamCases) expect(locked(row.update(step.text, step.final))).toEqual(step.expected as unknown)
  })

  test(`any chunking reaches the segmentCases states`, () => {
    for (const c of fixture.segmentCases) {
      for (const size of [1, 3, 17]) {
        const row = new ExpressRow(c.rowKey, sdk)
        let out: RowSegment[] | null = null
        for (let i = size; i < c.text.length + size; i += size) out = row.update(c.text.slice(0, i), false)
        out = row.update(c.text, c.final)
        expect(locked(out)).toEqual(c.expected as unknown)
      }
    }
  })
})

describe(`actionCases`, () => {
  for (const c of fixture.actionCases) {
    test(c.name, () => {
      expect(surfaceAction(c.event, c.origin as never) as unknown).toEqual(c.expected as unknown)
    })
  }
})

describe(`inputCases`, () => {
  for (const c of fixture.inputCases) {
    test(c.name, () => {
      expect(actionInput(c.card as ActionCard, c.body)).toBe(c.expected)
      expect(parseActionInput(c.expected)).toEqual({ card: c.card as ActionCard, body: c.body })
    })
  }
  test(`anything else is no action input`, () => {
    expect(parseActionInput(`[action] not json`)).toBeNull()
    expect(parseActionInput(`Ship it`)).toBeNull()
  })
})

describe(`failureNoteCases`, () => {
  for (const c of fixture.failureNoteCases) {
    test(c.name, () => {
      expect(failureNote(c.texts, sdk)).toBe(c.expected)
    })
  }
})

describe(`plainProseCases`, () => {
  for (const c of fixture.plainProseCases) {
    test(c.name, () => {
      expect(plainProse(c.text)).toBe(c.expected)
    })
  }
})

describe(`previewCases`, () => {
  for (const c of fixture.previewCases) {
    test(c.name, () => {
      const card = (c as { card?: Record<string, unknown> }).card ?? null
      const message = {
        id: `m1`,
        sessionId: `r`,
        authorKind: c.authorKind,
        authorId: null,
        authorSessionId: null,
        body: c.body,
        card,
        answeredAt: null,
        consumedAt: `2026-11-23T08:00:00.000Z`,
        createdAt: `2026-11-23T09:00:00.000Z`,
      }
      expect(chatPreview({ rootId: `r`, needsYou: false, busy: false, messages: [message as never] })).toEqual(c.expected)
    })
  }
})
