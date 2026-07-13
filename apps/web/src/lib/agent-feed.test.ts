import { describe, expect, it } from "vitest"
import {
  consumeEcho,
  ECHO_CAP,
  ECHO_TTL_MS,
  pushEcho,
  trailingQuestionIds,
  type EchoEntry,
} from "./agent-feed"

describe(`local-echo dedupe`, () => {
  it(`consumes a matching echo exactly once`, () => {
    const echoes: EchoEntry[] = []
    pushEcho(echoes, `fix the login bug`, 1_000)
    expect(consumeEcho(echoes, `fix the login bug`, 2_000)).toBe(true)
    // The second identical event (e.g. relay replay) is NOT swallowed.
    expect(consumeEcho(echoes, `fix the login bug`, 3_000)).toBe(false)
  })

  it(`matches on trimmed text`, () => {
    const echoes: EchoEntry[] = []
    pushEcho(echoes, `  hello  `, 0)
    expect(consumeEcho(echoes, `hello\n`, 1)).toBe(true)
  })

  it(`expired echoes never match`, () => {
    const echoes: EchoEntry[] = []
    pushEcho(echoes, `late message`, 0)
    expect(consumeEcho(echoes, `late message`, ECHO_TTL_MS + 1)).toBe(false)
    expect(echoes).toHaveLength(0)
  })

  it(`keeps at most ECHO_CAP entries`, () => {
    const echoes: EchoEntry[] = []
    for (let i = 0; i < ECHO_CAP + 3; i++) pushEcho(echoes, `msg ${i}`, i)
    expect(echoes).toHaveLength(ECHO_CAP)
    expect(consumeEcho(echoes, `msg 0`, 10)).toBe(false)
    expect(consumeEcho(echoes, `msg ${ECHO_CAP + 2}`, 10)).toBe(true)
  })
})

describe(`trailingQuestionIds`, () => {
  it(`returns the trailing consecutive question run`, () => {
    const feed = [
      { id: 1, kind: `narration` },
      { id: 2, kind: `question` },
      { id: 3, kind: `tool` },
      { id: 4, kind: `question` },
      { id: 5, kind: `question` },
    ]
    expect(trailingQuestionIds(feed)).toEqual(new Set([4, 5]))
  })

  it(`is empty when the feed ends with a non-question`, () => {
    const feed = [
      { id: 1, kind: `question` },
      { id: 2, kind: `narration` },
    ]
    expect(trailingQuestionIds(feed)).toEqual(new Set())
  })

  it(`handles an all-question feed and an empty feed`, () => {
    expect(
      trailingQuestionIds([
        { id: 1, kind: `question` },
        { id: 2, kind: `question` },
      ])
    ).toEqual(new Set([1, 2]))
    expect(trailingQuestionIds([])).toEqual(new Set())
  })
})
