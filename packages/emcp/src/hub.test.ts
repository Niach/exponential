// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/hub.json" with { type: "json" }
import { type HubGrant, type HubOutcome, type HubServer, hubAuditRow, hubRoute, hubServerNameValid, hubToolList } from "./hub"

const servers = fixture.servers as HubServer[]

describe(`hubServerNameValid`, () => {
  for (const entry of fixture.serverNameCases) {
    test(JSON.stringify(entry.name), () => expect(hubServerNameValid(entry.name)).toBe(entry.expected))
  }
})

describe(`hubToolList`, () => {
  for (const entry of fixture.listCases) {
    test(entry.name, () => {
      expect(hubToolList(entry.grant as HubGrant, servers).map((tool) => tool.name)).toEqual(entry.expected)
    })
  }
})

describe(`hub calls`, () => {
  for (const entry of fixture.callCases) {
    test(entry.name, () => {
      const grant = entry.grant as HubGrant
      const routed = hubRoute(grant, servers, entry.call.name)
      expect(routed).toEqual(entry.expected.routed)
      const outcome = (routed ? entry.upstream : `unknown`) as HubOutcome
      const row = hubAuditRow({
        id: `row-1`,
        grant,
        run: entry.run,
        name: entry.call.name,
        args: entry.call.arguments,
        outcome,
        latencyMs: 12.9,
        createdAtMs: 0,
      })
      const { id: _id, latencyMs, createdAt: _at, ...rest } = row
      expect(rest).toEqual(entry.expected.row as never)
      expect(latencyMs).toBe(12)
    })
  }
})

describe(`createdAt`, () => {
  for (const entry of fixture.createdAtCases) {
    test(entry.expected, () => {
      const row = hubAuditRow({
        id: `r`,
        grant: { servers: {}, teamId: null, userId: null },
        run: `run-1`,
        name: `x`,
        args: null,
        outcome: `unknown`,
        latencyMs: 0,
        createdAtMs: entry.ms,
      })
      expect(row.createdAt).toBe(entry.expected)
    })
  }
})
