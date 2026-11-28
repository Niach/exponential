// EXP-1324: the fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/trace.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import {
  computerToolCaption,
  parseTestResults,
  traceFailedTests,
  traceReasoning,
  traceSubagent,
  traceTool,
  type TraceToolInput,
} from "./trace"

describe(`traceTool`, () => {
  for (const entry of fixture.cases.tool) {
    test(entry.name, () => {
      expect(traceTool(entry.input as TraceToolInput)).toEqual(entry.expected as never)
    })
  }
})

// EDE-271: every computer-server row has both captions, and only the exact
// `mcp__computer__` name resolves.
describe(`computerToolCaption`, () => {
  test(`every row reads its contract captions`, () => {
    const { prefix, tools } = contract.computerToolDisplay
    expect(tools.length).toBeGreaterThan(0)
    for (const tool of tools) {
      expect(tool.progressive.trim()).not.toBe(``)
      expect(tool.done.trim()).not.toBe(``)
      expect(computerToolCaption(`${prefix}${tool.name}`, false)).toBe(tool.progressive)
      expect(computerToolCaption(` ${prefix}${tool.name} `, true)).toBe(tool.done)
    }
    expect(computerToolCaption(`mcp__computer__screenshot`, false)).toBe(`Taking screenshot`)
    expect(computerToolCaption(`mcp__computer__type`, true)).toBe(`Typed`)
  })
  test(`another server's or a bare name stays raw`, () => {
    for (const name of [`type`, `left_click`, `mcp__other__type`, `x_mcp__computer__type`, `mcp__computer__nope`, `mcp__computer__`, ``, null]) {
      expect(computerToolCaption(name, true)).toBeNull()
    }
  })
})

describe(`traceReasoning`, () => {
  for (const entry of fixture.cases.reasoning) {
    test(entry.name, () => {
      expect(traceReasoning(entry.input)).toEqual(entry.expected as never)
    })
  }
})

describe(`traceSubagent`, () => {
  for (const entry of fixture.cases.subagent) {
    test(entry.name, () => {
      expect(traceSubagent(entry.input)).toEqual(entry.expected as never)
    })
  }
})

describe(`parseTestResults + traceFailedTests`, () => {
  for (const entry of fixture.cases.tests) {
    test(entry.name, () => {
      const results = parseTestResults(entry.output)
      expect({ results, line: traceFailedTests(results) }).toEqual(entry.expected as never)
    })
  }
})

test(`every icon the fixture names is a trace concept`, () => {
  const icons = [
    ...Object.values(fixture.icons),
    ...Object.values(fixture.tool.verbs).map((verb) => verb.icon),
  ]
  for (const icon of icons) expect(icon.startsWith(`trace-`)).toBe(true)
})
