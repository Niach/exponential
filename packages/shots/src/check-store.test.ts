/* shots:check (#EXP-1779): a declared pair without its file is missing, a
   file no pair claims is undeclared, (the release suite runs it over shots/). */
import { afterEach, beforeEach, describe, expect, test } from "bun:test"
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { PLATFORMS, VIEWS, captureFor } from "@exp/view-catalog"
import { checkStore } from "./check-store.ts"

let dir: string
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), `exp-shots-check-`))
})
afterEach(() => rmSync(dir, { recursive: true, force: true }))

const view = VIEWS.find((v) => captureFor(v, `web`) !== undefined)!
const absent = PLATFORMS.find((p) => captureFor(view, p) === undefined)

describe(`checkStore`, () => {
  test(`an empty store owes every declared pair and holds nothing undeclared`, () => {
    const r = checkStore(dir)
    expect(r.captured).toEqual([])
    expect(r.missing).toContain(`${view.id}/web`)
    expect(r.undeclared).toEqual([])
  })

  test(`a captured pair is no longer missing; strays are undeclared`, () => {
    mkdirSync(join(dir, view.id), { recursive: true })
    writeFileSync(join(dir, view.id, `web.webp`), ``)
    if (absent) writeFileSync(join(dir, view.id, `${absent}.webp`), ``)
    mkdirSync(join(dir, `no-such-view`))
    writeFileSync(join(dir, `index.json`), `{}`)
    writeFileSync(join(dir, `stray.png`), ``)
    const r = checkStore(dir)
    expect(r.captured).toContain(`${view.id}/web`)
    expect(r.missing).not.toContain(`${view.id}/web`)
    expect(r.undeclared).toEqual([...(absent ? [`${view.id}/${absent}.webp`] : []), `no-such-view/`, `stray.png`].sort((a, b) => a.localeCompare(b)))
  })
})
