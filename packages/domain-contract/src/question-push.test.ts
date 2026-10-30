// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/question-push.json" with { type: "json" }
import { ASSISTANT_COPY } from "./agent-settings"
import { CHATS_COPY } from "./chats"
import {
  questionPushBody,
  questionPushRecipients,
  questionPushText,
  questionPushTitle,
} from "./question-push"

const hex = (text: string) => new Uint8Array(Buffer.from(text, `hex`))
const keyMap = (keys: Record<string, string> | null) =>
  keys ? new Map(Object.entries(keys).map(([epoch, secret]) => [Number(epoch), hex(secret)])) : null

describe(`question push (fixtures/question-push.json)`, () => {
  test.each(fixture.routingCases)(`routing: $name`, (c) => {
    expect(questionPushRecipients(c.session)).toEqual(c.recipients)
  })
  test(`its copy is the Assistant's decision push and the chats' ask leads`, () => {
    expect(fixture.copy.assistantTitle).toBe(ASSISTANT_COPY.decisionPush)
    expect(fixture.textCases[0]!.askText.startsWith(CHATS_COPY.preview.askQuestionLead)).toBe(true)
    expect(fixture.textCases[1]!.askText.startsWith(CHATS_COPY.preview.askPlanLead)).toBe(true)
  })
  test.each(fixture.titleCases)(`title: $title`, (c) => {
    expect(questionPushTitle(c)).toBe(c.title)
  })
  test.each(fixture.textCases)(`text: $name`, (c) => {
    expect(questionPushText(c.askText)).toBe(c.shown)
    expect([...c.shown].length).toBeLessThanOrEqual(fixture.bodyMax)
  })
  test.each(fixture.bodyCases)(`body: $name`, (c) => {
    const data = c.data as Record<string, string>
    const keys = keyMap(c.keys as Record<string, string> | null)
    expect(questionPushBody(data, (rootId) => (rootId === data.rootId ? keys : null), c.fallback)).toBe(c.shown)
  })
})
