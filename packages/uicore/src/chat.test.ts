import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/chat.json" with { type: "json" }
import { chatView, type ChatInput } from "./chat"

describe(`chat.json`, () => {
  test(`covers every face and row kind`, () => {
    expect(fixture.cases.length).toBeGreaterThanOrEqual(15)
    const kinds = new Set(fixture.cases.flatMap((c) => c.expected.rows.map((r) => r.kind)))
    expect([...kinds].sort()).toEqual([`agent`, `card`, `host`, `line`, `marker`, `picture`, `pill`, `question`, `user`])
  })
  for (const c of fixture.cases) {
    test(c.name, () => {
      expect(chatView({ ...fixture.base, ...c.input } as ChatInput)).toEqual(c.expected as ReturnType<typeof chatView>)
    })
  }
})
