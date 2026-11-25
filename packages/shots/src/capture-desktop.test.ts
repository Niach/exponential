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
  windowCapture,
  assertBinaryFromCheckout,
  binaryCheckout,
  desktopChangelogHeadId,
  desktopWaitsForFeed,
  makeDataDir,
} from "./capture-desktop.ts"
import { viewsFor } from "@exp/view-catalog"

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

  test(`#EXP-2121: the Assistant button already welcomed the signed-in member`, () => {
    const dir = makeDataDir(undefined, `u1`)
    try {
      const settings = JSON.parse(readFileSync(join(dir, `settings.json`), `utf8`))
      expect(settings.assistantFab).toEqual({ u1: { entered: true, welcomed: true, bubbled: [] } })
    } finally {
      rmSync(dir, { recursive: true, force: true })
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

describe(`windowCapture`, () => {
  // A macOS alert over the window must not land in the store: the window
  // alone is the default, the screen rect an opt-in.
  test(`the window alone unless SHOTS_DESKTOP_CAPTURE=rect`, () => {
    expect(windowCapture({})).toBe(true)
    expect(windowCapture({ SHOTS_DESKTOP_CAPTURE: `window` })).toBe(true)
    expect(windowCapture({ SHOTS_DESKTOP_CAPTURE: ` RECT ` })).toBe(false)
  })
})

/**
 * #EXP-1886: a chat's Chat tab is the steer feed. A capture that did not
 * wait for its first replay photographed "No live transcript". Only a chat
 * the stand-in desktop holds a recorded journal for ever paints one
 * (apps/web/scripts/lib/demo-stored-journals.ts: `$chat`, the cold-start
 * chat). A view of a chat without one (`$guideChat`) must NOT wait: its
 * capture never signals ready and fails after 90s.
 */
describe(`desktopWaitsForFeed`, () => {
  const onChat = (placeholder: string) =>
    viewsFor(`desktop`).filter(
      (view) =>
        view.desktop?.drive.kind === `screen` &&
        view.desktop.drive.value === `chat:${placeholder}` &&
        !view.desktop.env?.EXP_DEV_CHAT_CHILD
    )

  test(`the views of the chat with a recorded journal wait for the feed`, () => {
    expect(onChat(`$chat`).map((view) => view.id)).toContain(`chat-screen`)
    expect(onChat(`$chat`).filter((view) => !desktopWaitsForFeed(view.desktop!)).map((view) => view.id)).toEqual([])
  })

  test(`the views of a chat without a journal never wait for one`, () => {
    const journals = readFileSync(join(import.meta.dir, `../../../apps/web/scripts/lib/demo-stored-journals.ts`), `utf8`)
    expect(journals).toContain(`[DEMO_CHAT_ID]:`)
    expect(journals).not.toContain(`DEMO_GUIDE_CHAT_ID`)
    expect(onChat(`$guideChat`).map((view) => view.id)).toContain(`chat-replies`)
    expect(onChat(`$guideChat`).filter((view) => desktopWaitsForFeed(view.desktop!)).map((view) => view.id)).toEqual([])
  })

  test(`a session screen waits on its own; a settings page never`, () => {
    expect(desktopWaitsForFeed({ drive: { kind: `screen`, value: `session:$session` } })).toBe(true)
    expect(desktopWaitsForFeed({ drive: { kind: `screen`, value: `chat:$chat` }, env: { EXP_DEV_WAIT_FEED: `1` } })).toBe(true)
    expect(desktopWaitsForFeed({ drive: { kind: `settings`, value: `profile` } })).toBe(false)
  })
})
