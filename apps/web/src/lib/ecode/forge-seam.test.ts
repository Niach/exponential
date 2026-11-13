import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"
import { describe, expect, it } from "vitest"

// #EXP-1614: the forge seam holds. The PR model (`lib/pull-requests.ts`),
// the PR state writer (`integrations/pr-sync.ts`) and the merge train talk
// to a code host ONLY through `lib/ecode/forge.ts`; GitHub's PR calls live
// behind `githubForge`.

const SRC = join(__dirname, `..`, `..`)

/** Every non-test source file under `dir` (repo-relative to src/). */
function sources(dir: string): string[] {
  const out: string[] = []
  const walk = (at: string) => {
    for (const name of readdirSync(at)) {
      const path = join(at, name)
      if (statSync(path).isDirectory()) walk(path)
      else if (/\.(ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name) && !name.endsWith(`.gen.ts`)) {
        out.push(relative(SRC, path))
      }
    }
  }
  walk(join(SRC, dir))
  return out.sort()
}

/** The modules a file imports (static and dynamic). */
function importsOf(file: string): string[] {
  const text = readFileSync(join(SRC, file), `utf8`)
  return [...text.matchAll(/(?:from\s+|import\(\s*)["'`](@\/[^"'`]+)["'`]/g)].map((m) => m[1]!)
}

/** GitHub's integration modules, and the PR helpers built on them. */
const GITHUB = /^@\/lib\/(integrations\/(github-[a-z-]+|pr-files|pr-checks)|pr-stacks)$/
/** The PR-tree calls (open, list, retarget, merge, close, files, stacks). */
const FORGE_MODULES = /^@\/lib\/(integrations\/(github-pr|pr-files)|pr-stacks)$/

describe(`the forge seam (#EXP-1614)`, () => {
  it(`the PR model, the PR state writer and the merge train import no GitHub module`, () => {
    const seamed = [`lib/pull-requests.ts`, `lib/integrations/pr-sync.ts`, ...sources(`lib/merge-train`)]
    const offenders = seamed.flatMap((file) => importsOf(file).filter((m) => GITHUB.test(m)).map((m) => `${file} → ${m}`))
    expect(offenders).toEqual([])
  })

  it(`the PR-tree calls are reached only through lib/ecode (plus a list that only shrinks)`, () => {
    // Each of these moves behind the forge when its feature meets a second
    // provider (#EXP-1612); the list only ever shrinks.
    const NOT_YET = new Set([
      `lib/bootstrap-self-hosted.ts`, // the GITHUB_POLLING poller (GitHub-only by nature)
      `lib/pr-merge-guard.ts`,
      `lib/session-guide-diff.ts`,
      `lib/trpc/pr-update.ts`,
      `lib/trpc/repositories.ts`, // the GitHub repo picker (open PRs, branch check)
    ])
    const allowed = (file: string) =>
      file.startsWith(`lib/ecode/`) ||
      file === `routes/api/webhooks/github.ts` ||
      // GitHub's own integration modules build on each other.
      /^lib\/integrations\/(github-[a-z-]+|pr-files)\.ts$/.test(file) ||
      file === `lib/pr-stacks.ts` ||
      NOT_YET.has(file)
    const importers = [...sources(`lib`), ...sources(`routes`)].filter((file) =>
      importsOf(file).some((m) => FORGE_MODULES.test(m))
    )
    expect(importers.filter((file) => !allowed(file))).toEqual([])
    // A file that stopped importing them leaves the list.
    expect([...NOT_YET].filter((file) => !importers.includes(file))).toEqual([])
  })
})
