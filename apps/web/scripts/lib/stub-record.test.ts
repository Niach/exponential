import { mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { afterEach, describe, expect, it } from "vitest"
import { readStubRecord, releaseStubRecord, stubRecordPath, writeStubRecord } from "./stub-record"

const dirs: string[] = []
const scratch = () => {
  const dir = mkdtempSync(join(tmpdir(), `exp-stub-record-`))
  dirs.push(dir)
  return dir
}
afterEach(() => {
  for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true })
})

const record = (pid: number, online = false) => ({ pid, relayUrl: `ws://localhost:4002`, databaseUrl: `postgres://x`, online })

describe(`the screenshot stub's record (#EXP-1886)`, () => {
  it(`a second stub's exit leaves the first stub's record in place`, () => {
    const file = stubRecordPath(`/w`, scratch())
    writeStubRecord(file, record(100))
    expect(releaseStubRecord(file, 200)).toBe(false)
    expect(readStubRecord(file)?.pid).toBe(100)
    expect(releaseStubRecord(file, 100)).toBe(true)
    expect(readStubRecord(file)).toBeUndefined()
  })

  it(`is written while starting and flips to online`, () => {
    const file = stubRecordPath(`/w`, scratch())
    writeStubRecord(file, record(100))
    expect(readStubRecord(file)?.online).toBe(false)
    writeStubRecord(file, record(100, true))
    expect(readStubRecord(file)?.online).toBe(true)
  })

  it(`reads an older record (written only once online) as online`, () => {
    const file = stubRecordPath(`/w`, scratch())
    writeFileSync(file, JSON.stringify({ pid: 7, relayUrl: `ws://localhost:4002`, databaseUrl: `` }))
    expect(readStubRecord(file)).toEqual({ pid: 7, relayUrl: `ws://localhost:4002`, databaseUrl: ``, online: true })
  })
})
