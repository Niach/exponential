// The fixtures are the contract; every mirror replays them.
import { describe, expect, test } from "bun:test"

import appRecords from "@exp/domain-contract/fixtures/app-records.json" with { type: "json" }

import capabilities from "../fixtures/capabilities.json" with { type: "json" }
import consent from "../fixtures/consent.json" with { type: "json" }
import lifecycle from "../fixtures/lifecycle.json" with { type: "json" }
import outbox from "../fixtures/outbox.json" with { type: "json" }
import providers from "../fixtures/providers.json" with { type: "json" }
import {
  agentToolList,
  brokerAuditRow,
  brokerDecide,
  capabilityNameValid,
  consentAnswer,
  consentSheet,
  engineRecordRefusal,
  engineViewer,
  grantsOnPackage,
  grantsRecord,
  grantWrite,
  lifecycleStep,
  manageSheet,
  networkAllows,
  outboxMode,
  outboxRows,
  parseCapabilityCall,
  scopeIdValid,
  STOPPED,
  validateHostConfig,
  type BrokerInput,
  type BrokerVerdict,
  type ConsentAnswer,
  type EngineIdentity,
  type Grants,
  type LifecycleEvent,
  type LifecycleState,
  type ManifestEntry,
} from "./index"

describe(`capabilities`, () => {
  test(`the catalog is well formed`, () => {
    for (const c of capabilities.capabilities) {
      expect(capabilityNameValid(c.name)).toBe(true)
      for (const s of c.scopes) {
        expect(scopeIdValid(s.id)).toBe(true)
        expect(s.id.startsWith(`${c.name}.`)).toBe(true)
        expect(s.tools).toEqual([...s.tools].sort())
      }
    }
    for (const name of capabilities.reservedNames) expect(capabilityNameValid(name)).toBe(false)
  })
  for (const c of capabilities.parseCases) {
    test(`parse ${c.from} ${c.name}`, () => expect(parseCapabilityCall(c.from as `ui` | `agent`, c.name)).toEqual(c.expected))
  }
  for (const c of capabilities.decideCases) {
    test(`decide: ${c.name}`, () => expect(brokerDecide(c as unknown as BrokerInput)).toEqual(c.expected as BrokerVerdict))
  }
  for (const c of capabilities.listCases) {
    test(`list: ${c.name}`, () => expect(agentToolList(c.manifest, c.provided)).toEqual(c.expected))
  }
  for (const c of capabilities.auditCases) {
    test(`audit: ${c.name}`, () => {
      expect(brokerAuditRow({ appId: c.appId, client: c.client, call: c.call as BrokerInput[`call`], verdict: c.verdict as BrokerVerdict, outcome: c.outcome as never })).toEqual(c.expected as never)
    })
  }
})

describe(`consent`, () => {
  for (const c of consent.answerCases) {
    test(`answer ${c.answer}`, () => expect(consentAnswer(c.grants as Grants, c.scope, c.answer as ConsentAnswer)).toEqual(c.expected as never))
  }
  for (const c of consent.writeCases) {
    test(`write: ${c.name}`, () => expect(grantWrite(c.manifest, c.grants as Grants, c.write as never)).toEqual(c.expected as never))
  }
  for (const c of consent.storeCases) {
    test(`store: ${c.name}`, () => {
      const record = grantsRecord(c.app, c.signer, c.manifest, c.grants as Grants)
      expect(record).toEqual(c.expected)
      expect(grantsOnPackage(record.value, c.signer, c.manifest)).toEqual(c.grants as Grants)
    })
  }
  for (const c of consent.reconcileCases) {
    test(`reconcile: ${c.name}`, () => expect(grantsOnPackage(c.stored, c.signer, c.manifest as ManifestEntry[])).toEqual(c.expected as Grants))
  }
  for (const c of consent.sheetCases) {
    test(`sheet: ${c.name}`, () => expect(consentSheet(c.app, c.entry)).toEqual({ components: c.expected, data: c.data } as never))
  }
  for (const c of consent.manageCases) {
    test(`manage: ${c.name}`, () => expect(manageSheet(c.manifest, c.grants as Grants)).toEqual(c.expected as never))
  }
})

describe(`lifecycle`, () => {
  for (const c of lifecycle.cases) {
    test(c.name, () => {
      let state: LifecycleState = STOPPED
      for (const step of c.steps) {
        const next = lifecycleStep(state, step.event as LifecycleEvent, { pkg: lifecycle.package, lastScreen: c.lastScreen })
        expect(next.effects).toEqual(step.effects as never)
        expect(next.state).toEqual(step.state as LifecycleState)
        state = next.state
      }
    })
  }
})

describe(`providers`, () => {
  for (const c of providers.validateCases) {
    test(`validate: ${c.name}`, () => expect(validateHostConfig(c.config)).toEqual(c.expected as never))
  }
  for (const [host, config] of Object.entries(providers.hosts)) {
    test(`host ${host} passes`, () => expect(validateHostConfig(config)).toEqual([]))
  }
  for (const c of providers.identityCases) {
    test(`identity: ${c.name}`, () => {
      const identity = c.identity as EngineIdentity
      expect(engineViewer(identity)).toEqual(c.expected.viewer as never)
      expect(engineRecordRefusal(identity, c.policy as never, c.op as never, c.row)).toBe(c.expected.refusal)
    })
  }
  for (const c of providers.networkCases) {
    test(`network ${c.policy} ${c.target} ${c.declared}`, () => expect(networkAllows(c.policy as never, c.target as never, c.declared)).toEqual(c.expected))
  }
})

describe(`outbox`, () => {
  test(`the $ref names O1's section`, () => {
    expect(outbox.$ref).toBe(`../../domain-contract/fixtures/app-records.json#/outbox`)
    expect(appRecords.outbox.rowsCases.length).toBeGreaterThan(0)
  })
  test(`O1's rows replay through the Engine's re-export`, () => {
    for (const c of appRecords.outbox.rowsCases) expect(outboxRows(c.rows as never, c.entries as never, c.me)).toEqual(c.expected as never)
  })
  for (const c of outbox.modeCases) test(`mode: ${c.name}`, () => expect(outboxMode(c.sync)).toBe(c.expected as never))
})
