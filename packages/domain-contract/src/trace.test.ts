// EXP-1324: the fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/trace.json" with { type: "json" }
import {
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
