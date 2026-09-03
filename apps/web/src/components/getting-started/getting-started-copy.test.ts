// EXP-698 r5 — the getting-started checklist ships on all four clients, and a
// checklist that says something different on each one is worse than no
// checklist. Web's table (`getting-started-copy.ts`) is the source; this test
// reads the three native copy files off disk and asserts every title,
// description and action label appears in them as a string literal.
//
// The natives hold their own literals rather than a generated file on purpose:
// three tiny constant tables cost less than a fifth code generator, and this
// gate is what keeps them honest. A native file that is missing (a fresh
// checkout of a platform that has not landed yet) fails LOUDLY — that is the
// drift the test exists to catch.
import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  GETTING_STARTED_COPY,
  MOBILE_GETTING_STARTED_KEYS,
} from "@/components/getting-started/getting-started-copy"
import type { EntryKey } from "@/components/getting-started/getting-started-model"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`, `..`)

const DESKTOP = `apps/desktop/crates/ui/src/getting_started.rs`
const IOS = `apps/ios/Exponential/UI/GettingStarted/GettingStartedCopy.swift`
const ANDROID = `apps/android/app/src/main/java/com/exponential/app/ui/gettingstarted/GettingStartedCopy.kt`

const ALL_KEYS = Object.keys(GETTING_STARTED_COPY) as EntryKey[]

function read(relative: string): string {
  return readFileSync(join(repoRoot, relative), `utf8`)
}

/** Every string the platform has to carry, for the keys it renders. */
function strings(keys: readonly EntryKey[]): { key: EntryKey; value: string }[] {
  return keys.flatMap((key) => {
    const entry = GETTING_STARTED_COPY[key]
    return [entry.title, entry.description, entry.action]
      .filter((value) => value.length > 0)
      .map((value) => ({ key, value }))
  })
}

describe(`getting-started copy`, () => {
  it(`keeps the strings quotable in Swift, Kotlin and Rust source`, () => {
    for (const { value } of strings(ALL_KEYS)) {
      expect(value).not.toContain(`"`)
      expect(value).not.toContain(`\\`)
      // eslint-disable-next-line no-control-regex
      expect(/^[\x20-\x7e]+$/.test(value) ? value : `non-ASCII: ${value}`).toBe(
        value
      )
    }
  })

  it(`the desktop IDE carries all ten entries verbatim`, () => {
    const src = read(DESKTOP)
    for (const { key, value } of strings(ALL_KEYS)) {
      expect(
        src.includes(`"${value}"`) ? value : `${DESKTOP} is missing ${key}: ${value}`
      ).toBe(value)
    }
  })

  it(`iOS carries the seven mobile entries verbatim`, () => {
    const src = read(IOS)
    for (const { key, value } of strings(MOBILE_GETTING_STARTED_KEYS)) {
      expect(
        src.includes(`"${value}"`) ? value : `${IOS} is missing ${key}: ${value}`
      ).toBe(value)
    }
  })

  it(`Android carries the seven mobile entries verbatim`, () => {
    const src = read(ANDROID)
    for (const { key, value } of strings(MOBILE_GETTING_STARTED_KEYS)) {
      expect(
        src.includes(`"${value}"`)
          ? value
          : `${ANDROID} is missing ${key}: ${value}`
      ).toBe(value)
    }
  })

  it(`hides only the three web-managed entries from the phones`, () => {
    const hidden = ALL_KEYS.filter(
      (key) => !MOBILE_GETTING_STARTED_KEYS.includes(key)
    )
    expect(hidden).toEqual([`widget`, `helpdesk`, `mcp`])
  })
})
