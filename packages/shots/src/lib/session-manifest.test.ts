import { describe, expect, test } from "bun:test"
import { mkdtempSync, readFileSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { sessionManifestAdd, sessionManifestPath } from "./session-manifest.ts"

describe(`session manifest (#EXP-1868)`, () => {
  test(`a capture run's temp dir lands in its session's manifest, once`, () => {
    const dir = mkdtempSync(join(tmpdir(), `exp-manifest-ts-`))
    const env = { EXP_MCP_SESSION_ID: `81457d90-7766-4380-b5c3-117b07633c12`, EXP_SESSION_MANIFEST_DIR: dir }
    expect(sessionManifestAdd(`dir`, `/tmp/exp-shots-desktop-abc`, env)).toBe(true)
    expect(sessionManifestAdd(`dir`, `/tmp/exp-shots-desktop-abc`, env)).toBe(true)
    expect(sessionManifestAdd(`sim`, `A1B2C3D4-0000-4000-8000-00000000E868`, env)).toBe(true)
    expect(readFileSync(join(dir, env.EXP_MCP_SESSION_ID), `utf8`)).toBe(
      `dir\t/tmp/exp-shots-desktop-abc\nsim\tA1B2C3D4-0000-4000-8000-00000000E868\n`
    )
    rmSync(dir, { recursive: true, force: true })
  })

  test(`outside a session, or a value that is not one line, nothing is recorded`, () => {
    expect(sessionManifestPath({})).toBeNull()
    expect(sessionManifestAdd(`dir`, `/tmp/x`, {})).toBe(false)
    const env = { EXP_MCP_SESSION_ID: `s1`, EXP_SESSION_MANIFEST_DIR: `/nonexistent-81457` }
    expect(sessionManifestAdd(`dir`, `relative/x`, env)).toBe(false)
    expect(sessionManifestAdd(`compose`, `a\tb`, env)).toBe(false)
    expect(sessionManifestPath({ EXP_MCP_SESSION_ID: `../etc` })).toBeNull()
  })
})
