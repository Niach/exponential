import { describe, expect, it } from "vitest"
import {
  consumeEcho,
  ECHO_CAP,
  ECHO_TTL_MS,
  groupToolRuns,
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

  it(`is unaffected by tool runs preceding the trailing questions`, () => {
    const feed = [
      { id: 1, kind: `tool` },
      { id: 2, kind: `tool` },
      { id: 3, kind: `question` },
    ]
    expect(trailingQuestionIds(feed)).toEqual(new Set([3]))
  })
})

describe(`groupToolRuns`, () => {
  const item = (id: number, kind: string) => ({ id, kind })

  it(`collapses runs of >=2 consecutive tools, leaves everything else single`, () => {
    const feed = [
      item(1, `narration`),
      item(2, `tool`),
      item(3, `tool`),
      item(4, `tool`),
      item(5, `user_message`),
      item(6, `tool`),
    ]
    expect(groupToolRuns(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `toolRun`, id: 2, items: [feed[1], feed[2], feed[3]] },
      { kind: `single`, item: feed[4] },
      { kind: `single`, item: feed[5] },
    ])
  })

  it(`a lone tool between other kinds stays a single row`, () => {
    const feed = [item(1, `tool`), item(2, `narration`), item(3, `tool`)]
    expect(groupToolRuns(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `single`, item: feed[1] },
      { kind: `single`, item: feed[2] },
    ])
  })

  it(`two runs split by a narration stay separate runs`, () => {
    const feed = [
      item(1, `tool`),
      item(2, `tool`),
      item(3, `narration`),
      item(4, `tool`),
      item(5, `tool`),
    ]
    expect(groupToolRuns(feed)).toEqual([
      { kind: `toolRun`, id: 1, items: [feed[0], feed[1]] },
      { kind: `single`, item: feed[2] },
      { kind: `toolRun`, id: 4, items: [feed[3], feed[4]] },
    ])
  })

  it(`an all-tool feed is one run; an empty feed has no rows`, () => {
    const feed = [item(1, `tool`), item(2, `tool`), item(3, `tool`)]
    expect(groupToolRuns(feed)).toEqual([{ kind: `toolRun`, id: 1, items: feed }])
    expect(groupToolRuns([])).toEqual([])
  })

  it(`run id stays the FIRST tool's id as the trailing run grows`, () => {
    const feed = [item(1, `narration`), item(2, `tool`), item(3, `tool`)]
    const before = groupToolRuns(feed)
    const after = groupToolRuns([...feed, item(4, `tool`)])
    expect(before[1]).toMatchObject({ kind: `toolRun`, id: 2 })
    expect(after[1]).toMatchObject({ kind: `toolRun`, id: 2 })
    expect((after[1] as { items: unknown[] }).items).toHaveLength(3)
  })

  it(`questions adjacent to tools are never absorbed into a run`, () => {
    const feed = [
      item(1, `tool`),
      item(2, `tool`),
      item(3, `question`),
      item(4, `question`),
    ]
    expect(groupToolRuns(feed)).toEqual([
      { kind: `toolRun`, id: 1, items: [feed[0], feed[1]] },
      { kind: `single`, item: feed[2] },
      { kind: `single`, item: feed[3] },
    ])
  })
})
