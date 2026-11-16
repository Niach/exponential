import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"

// #EXP-1838: the release scripts run the duplicate-share check as SQL on the
// DB host (scripts/release/shares.sql), so it must stay the query
// duplicateDeviceShares runs.
const root = resolve(__dirname, `../../../..`)
const squash = (text: string) => text.replace(/\s+/g, ` `).trim()

describe(`scripts/release/shares.sql`, () => {
  it(`is the query of duplicateDeviceShares`, () => {
    const source = readFileSync(resolve(root, `apps/web/src/lib/device-share-duplicates.ts`), `utf8`)
    const query = source.match(/execute\(sql`([\s\S]*?)`\)/)?.[1]
    expect(query).toBeTruthy()
    const shell = readFileSync(resolve(root, `scripts/release/shares.sql`), `utf8`)
    expect(squash(shell)).toBe(squash(query!))
  })
})
