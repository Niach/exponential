import { statfsSync } from "node:fs"
import { mkdir, rm, stat } from "node:fs/promises"
import { join } from "node:path"
import { simpleGit } from "simple-git"
import type { CompanionConfig } from "./config"
import type { Logger } from "./logger"

export interface WorktreeClaim {
  worktreePath: string
  branch: string
  repoPath: string
  defaultBranch: string
}

export interface WorktreeManager {
  claim(args: {
    repoPath: string
    defaultBranch: string
    identifier: string
    slug: string
  }): Promise<WorktreeClaim>
  cleanup(claim: WorktreeClaim): Promise<void>
}

function slugify(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
    .slice(0, 40)
}

async function bytesFree(path: string): Promise<number> {
  try {
    const s = statfsSync(path)
    return s.bsize * s.bavail
  } catch {
    return Number.MAX_SAFE_INTEGER
  }
}

export function createWorktreeManager(args: {
  config: CompanionConfig
  log: Logger
}): WorktreeManager {
  const { config, log } = args

  return {
    async claim({ repoPath, defaultBranch, identifier, slug }) {
      await mkdir(config.worktrees.root, { recursive: true })

      const free = await bytesFree(config.worktrees.root)
      if (free < config.worktrees.minFreeBytes) {
        throw new Error(
          `Worktree root ${config.worktrees.root} has only ${free} bytes free (< ${config.worktrees.minFreeBytes})`
        )
      }

      const worktreePath = join(config.worktrees.root, identifier)
      const branch = `${config.worktrees.branchPrefix}/${identifier.toLowerCase()}-${slugify(slug)}`

      // If a stale worktree exists, force-remove it first.
      try {
        await stat(worktreePath)
        log.warn({ worktreePath }, `removing stale worktree`)
        const g = simpleGit(repoPath)
        await g
          .raw([`worktree`, `remove`, `--force`, worktreePath])
          .catch(() => {})
        await rm(worktreePath, { recursive: true, force: true })
      } catch {
        // doesn't exist, good
      }

      const git = simpleGit(repoPath)
      // The clone is kept fresh by repo-manager.ts; we still fetch here as a
      // belt-and-braces guard against concurrent worktrees racing the fetch.
      await git.fetch(`origin`, defaultBranch)
      // -B (force-create) rather than -b: the previous run may have left the
      // branch behind (e.g. plan-mode created the worktree+branch, the
      // worktree was removed but the branch lingers; on the next code-mode
      // run -b would fail with "branch already exists"). -B resets the
      // existing branch to origin/<default> which is what we want anyway.
      await git.raw([
        `worktree`,
        `add`,
        `-B`,
        branch,
        worktreePath,
        `origin/${defaultBranch}`,
      ])
      log.info({ worktreePath, branch }, `worktree created`)

      return {
        worktreePath,
        branch,
        repoPath,
        defaultBranch,
      }
    },

    async cleanup(claim) {
      // Belt-and-suspenders: only remove branches matching the agent prefix.
      if (!claim.branch.startsWith(`${config.worktrees.branchPrefix}/`)) {
        log.warn(
          { branch: claim.branch },
          `refusing to clean branch outside agent prefix`
        )
        return
      }
      const git = simpleGit(claim.repoPath)
      await git
        .raw([`worktree`, `remove`, `--force`, claim.worktreePath])
        .catch((e: unknown) =>
          log.warn(
            { err: e instanceof Error ? e.message : String(e) },
            `worktree remove failed; will rm -rf the dir`
          )
        )
      await rm(claim.worktreePath, { recursive: true, force: true })
      await git
        .raw([`branch`, `-D`, claim.branch])
        .catch((e: unknown) =>
          log.warn(
            {
              err: e instanceof Error ? e.message : String(e),
              branch: claim.branch,
            },
            `branch -D failed`
          )
        )
      log.info({ branch: claim.branch }, `worktree cleaned up`)
    },
  }
}
