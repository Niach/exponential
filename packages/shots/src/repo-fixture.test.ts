/**
 * EXP-1558: the desktop lane's own clone — the same history, worktrees and
 * working-tree edits on every Mac, dated relative to the run.
 */
import { describe, expect, test } from "bun:test"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { buildRepoFixture } from "./repo-fixture.ts"

const git = (cwd: string, ...args: string[]) =>
  Bun.spawnSync({ cmd: [`git`, ...args], cwd, stdout: `pipe` }).stdout.toString().trim()

describe(`buildRepoFixture`, () => {
  test(`a trunk clone with a merged branch, two kept worktrees and uncommitted edits`, { timeout: 30_000 }, () => {
    const base = mkdtempSync(join(tmpdir(), `exp-shots-fixture-`))
    try {
      const now = Date.parse(`2026-11-02T12:00:00Z`)
      const root = buildRepoFixture(base, now)
      expect(root).toBe(join(base, `repos`))
      const clone = join(root, `acme/mobile-app`)
      const log = git(clone, `log`, `--format=%an|%cI|%s`, `main`).split(`\n`)
      expect(log[0]).toBe(`Alex Carter|2026-11-01T10:00:00Z|Profile app launch end-to-end`)
      expect(log.some((line) => line.endsWith(`Merge pull request #12 from acme/offline-queue`))).toBe(true)
      expect(git(clone, `status`, `--porcelain`).split(`\n`).length).toBe(2)
      expect(git(clone, `rev-parse`, `main`)).toBe(git(clone, `rev-parse`, `origin/main`))
      const worktrees = git(clone, `worktree`, `list`, `--porcelain`)
      expect(worktrees).toContain(`branch refs/heads/exp-session-9497089a`)
      expect(worktrees).toContain(`branch refs/heads/exp-EXP-1375`)
      // Rebuilding over the last run starts from the same tree.
      expect(buildRepoFixture(base, now)).toBe(root)
      expect(git(clone, `log`, `--format=%s`, `-1`, `main`)).toBe(`Profile app launch end-to-end`)
    } finally {
      rmSync(base, { recursive: true, force: true })
    }
  })
})
