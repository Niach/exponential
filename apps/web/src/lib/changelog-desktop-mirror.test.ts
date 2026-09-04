/**
 * EXP-723: the desktop IDE shows the same "What's new" card the web sidebar
 * does, but gpui cannot import a TypeScript array — so `crates/ui/changelog.rs`
 * hand-mirrors the HEAD entry, and `changelog_seen_id` in settings.json keys
 * the dismissal off its id.
 *
 * A mirror nobody checks is a mirror that goes stale on the very next release:
 * the web card would re-surface and the desktop one would not. This test is
 * that check. It reads the Rust file as TEXT (there is no Rust toolchain in
 * the web suite) and pins the two fields the dismissal and the heading depend
 * on. Everything else in the entry is prose the desktop is free to wrap
 * differently.
 */
import { existsSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"
import { CHANGELOG } from "@/lib/changelog"

const MIRROR = resolve(
  import.meta.dirname,
  `../../../../apps/desktop/crates/ui/src/changelog.rs`
)

const present = existsSync(MIRROR)
const describeMirror = present ? describe : describe.skip

// While the Rust side does not exist yet the gate has nothing to compare
// against. It must not fail the web suite for that — but it must not pass
// silently either, so it SKIPS with the reason in the title.
describeMirror(
  present
    ? `desktop changelog mirror`
    : `desktop changelog mirror (skipped: ${MIRROR} does not exist yet)`,
  () => {
    const source = present ? readFileSync(MIRROR, `utf8`) : ``
    const head = CHANGELOG[0]!

    // Tolerant on purpose: the Rust literal may be a `ChangelogEntry { … }`
    // struct, a const, or wrapped in a `LazyLock` — all that matters is that
    // the FIRST `id:`/`title:` string in the file is the head entry's.
    function firstField(name: string): string | null {
      const match = source.match(new RegExp(`${name}:\\s*"([^"]+)"`))
      return match ? match[1]! : null
    }

    it(`carries the head entry's id`, () => {
      expect(firstField(`id`)).toBe(head.id)
    })

    it(`carries the head entry's title`, () => {
      expect(firstField(`title`)).toBe(head.title)
    })

    it(`names the entry LATEST, which is what the rail renders`, () => {
      expect(source).toContain(`LATEST`)
    })
  }
)
