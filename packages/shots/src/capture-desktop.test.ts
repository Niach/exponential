/**
 * EXP-1308: every desktop capture starts with the sidebar "What's new" card
 * dismissed, the state the web lane pre-seeds, so the two never differ by a
 * per-release banner.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync, rmSync } from "node:fs"
import { join } from "node:path"
import { latestChangelogEntry } from "../../../apps/web/src/lib/changelog.ts"
import { desktopChangelogHeadId, makeDataDir } from "./capture-desktop.ts"

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
