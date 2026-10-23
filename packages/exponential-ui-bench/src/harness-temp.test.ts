// VAPP-150: every web harness build (the bench runners, the browser suites,
// the parity gate) leaves nothing behind in the system temp folder. Each
// build runs with TMPDIR pointed at a fresh, empty directory, which must
// still be empty afterwards.

import { describe, expect, it } from "bun:test"
import { mkdtempSync, readdirSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { bundleHarness as element } from "../../exponential-ui-element/harness/serve"
import { bundleHarness as react } from "../../exponential-ui-react/harness/serve"
import { bundleHarness as solid } from "../../exponential-ui-solid/harness/serve"

async function leftovers(build: () => Promise<string>): Promise<{ js: string; left: string[] }> {
  const scratch = mkdtempSync(join(tmpdir(), `xui-harness-temp-test-`))
  const before = process.env.TMPDIR
  process.env.TMPDIR = scratch
  try {
    expect(tmpdir()).toBe(scratch)
    const js = await build()
    return { js, left: readdirSync(scratch) }
  } finally {
    if (before === undefined) delete process.env.TMPDIR
    else process.env.TMPDIR = before
    rmSync(scratch, { recursive: true, force: true })
  }
}

describe(`harness builds leave the temp folder as they found it`, () => {
  for (const [name, build] of [[`solid`, solid], [`element`, element], [`react`, react]] as const) {
    it(
      name,
      async () => {
        const { js, left } = await leftovers(build)
        expect(js.length).toBeGreaterThan(10_000)
        expect(left).toEqual([])
      },
      180_000
    )
  }
})
