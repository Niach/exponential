/**
 * EXP-1308: every desktop capture starts with the sidebar "What's new" card
 * dismissed, the state the web lane pre-seeds, so the two never differ by a
 * per-release banner.
 */
import { describe, expect, test } from "bun:test"
import { mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { latestChangelogEntry } from "../../../apps/web/src/lib/changelog.ts"
import {
  assertBinaryFromCheckout,
  binaryCheckout,
  desktopChangelogHeadId,
  makeDataDir,
} from "./capture-desktop.ts"

describe(`makeDataDir`, () => {
  test(`pre-dismisses the head changelog entry, with or without a repos root`, () => {
    for (const reposRoot of [undefined, `/tmp/repos`]) {
      const dir = makeDataDir(reposRoot)
      try {
        const settings = JSON.parse(readFileSync(join(dir, `settings.json`), `utf8`))
        expect(settings).toEqual({
          ...(reposRoot ? { reposRoot } : {}),
          changelogSeenId: desktopChangelogHeadId(),
        })
      } finally {
        rmSync(dir, { recursive: true, force: true })
      }
    }
  })

  test(`the desktop head id is the web head id`, () => {
    expect(desktopChangelogHeadId()).toBe(latestChangelogEntry()!.id)
  })
})

/**
 * EXP-1558: the shared CARGO_TARGET_DIR holds whichever worktree linked last;
 * the lane refuses a binary that names another checkout or predates an edit.
 */
describe(`assertBinaryFromCheckout`, () => {
  const literal = (dir: string) =>
    new TextEncoder().encode(`\0rodata exp-built-from:${dir}/apps/desktop/crates/appExponential desktop {} starting`)

  const checkout = () => {
    const root = mkdtempSync(join(tmpdir(), `exp-shots-checkout-`))
    mkdirSync(join(root, `apps/desktop/crates/ui/src`), { recursive: true })
    writeFileSync(join(root, `apps/desktop/Cargo.lock`), ``)
    writeFileSync(join(root, `apps/desktop/crates/ui/src/lib.rs`), ``)
    const binary = join(root, `exp-desktop`)
    writeFileSync(binary, ``)
    // The sources an hour older than the binary.
    const past = new Date(Date.now() - 3_600_000)
    utimesSync(join(root, `apps/desktop/Cargo.lock`), past, past)
    utimesSync(join(root, `apps/desktop/crates/ui/src/lib.rs`), past, past)
    return { root, binary }
  }

  test(`reads the checkout out of packed string literals`, () => {
    expect(binaryCheckout(literal(`/w/one`))).toBe(`/w/one`)
    expect(binaryCheckout(new TextEncoder().encode(`no marker here`))).toBeUndefined()
  })

  test(`passes this checkout's fresh build, refuses another worktree's`, () => {
    const { root, binary } = checkout()
    try {
      expect(() => assertBinaryFromCheckout(binary, root, literal(root))).not.toThrow()
      expect(() => assertBinaryFromCheckout(binary, root, literal(`/elsewhere`))).toThrow(/built from \/elsewhere, not this checkout/)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  test(`refuses a binary older than a desktop source edit`, () => {
    const { root, binary } = checkout()
    try {
      const old = new Date(Date.now() - 7_200_000)
      utimesSync(binary, old, old)
      expect(() => assertBinaryFromCheckout(binary, root, literal(root))).toThrow(/older than this checkout's newest desktop source/)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})
