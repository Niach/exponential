import { readdirSync, readFileSync, statSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import { BOARD_REPO_NOTE } from "./board-copy"

// The note under the board form's repository/branch block is ONE string on
// every client (EXP-712). The natives carry it as a literal; this walks
// their UI sources so a reword on one platform fails here, not in review.
const ROOT = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const NATIVE_TREES: Record<string, { dir: string; ext: string }> = {
  ios: { dir: `apps/ios/Exponential/UI`, ext: `.swift` },
  android: { dir: `apps/android/app/src/main/java`, ext: `.kt` },
  desktop: { dir: `apps/desktop/crates/ui/src`, ext: `.rs` },
}

function walk(dir: string, ext: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry)
    if (statSync(path).isDirectory()) walk(path, ext, out)
    else if (path.endsWith(ext)) out.push(path)
  }
  return out
}

describe(`board repo note parity`, () => {
  for (const [platform, { dir, ext }] of Object.entries(NATIVE_TREES)) {
    it(`${platform} renders the exact note`, () => {
      const hits = walk(join(ROOT, dir), ext).filter((file) =>
        readFileSync(file, `utf8`).includes(BOARD_REPO_NOTE)
      )
      expect(hits.length, `no ${ext} file under ${dir} contains the note`)
        .toBeGreaterThan(0)
    })
  }
})
