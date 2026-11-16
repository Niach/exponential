import { describe, expect, test } from "bun:test"
import { isolatedStack, relayHealthUrl, relayUrl, stubState } from "./stack.ts"

describe(`the relay the preflight probes`, () => {
  test(`the default stack's relay`, () => {
    expect(relayUrl({})).toBe(`ws://localhost:4002`)
    expect(relayHealthUrl(relayUrl({}))).toBe(`http://localhost:4002/healthz`)
  })
  test(`an isolated stack's own relay, wherever it runs (not only compose)`, () => {
    expect(relayHealthUrl(relayUrl({ SHOTS_RELAY_URL: ` ws://localhost:4003 ` }))).toBe(`http://localhost:4003/healthz`)
    expect(relayHealthUrl(`wss://relay.example/ws?x=1`)).toBe(`https://relay.example/healthz`)
  })
})

describe(`isolatedStack`, () => {
  test(`only a proxy other than the default one`, () => {
    expect(isolatedStack({})).toBe(false)
    expect(isolatedStack({ SHOTS_PROXY_URL: `https://localhost:3000/` })).toBe(false)
    expect(isolatedStack({ SHOTS_PROXY_URL: `https://localhost:3001` })).toBe(true)
  })
})

describe(`stubState (#EXP-1886)`, () => {
  const here = { relayUrl: `ws://localhost:4002`, databaseUrl: `postgres://a`, alive: (pid: number) => pid === 100 }
  const record = { pid: 100, relayUrl: `ws://localhost:4002`, databaseUrl: `postgres://a`, online: false }
  test(`a stub still starting is waited for, never doubled`, () => {
    expect(stubState(record, here)).toBe(`starting`)
    expect(stubState({ ...record, online: true }, here)).toBe(`online`)
  })
  test(`no record, a dead pid, another relay or database = start one`, () => {
    expect(stubState(undefined, here)).toBe(`none`)
    expect(stubState({ ...record, pid: 200 }, here)).toBe(`none`)
    expect(stubState({ ...record, relayUrl: `ws://localhost:4003` }, here)).toBe(`none`)
    expect(stubState({ ...record, databaseUrl: `postgres://b` }, here)).toBe(`none`)
  })
})
