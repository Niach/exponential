import { mkdir, stat } from "node:fs/promises"
import { join } from "node:path"
import { spawn } from "node:child_process"
import { homedir } from "node:os"
import type { Logger } from "./logger"

const REPOS_ROOT = join(homedir(), `.exponential-companion`, `repos`)

export interface RepoHandle {
  /** Local clone path, e.g. ~/.exponential-companion/repos/owner/repo */
  repoPath: string
  owner: string
  repo: string
  defaultBranch: string
}

function parseOwnerRepo(ownerRepo: string): { owner: string; repo: string } {
  const m = ownerRepo.match(/^([A-Za-z0-9._-]+)\/([A-Za-z0-9._-]+)$/)
  if (!m) {
    throw new Error(
      `Invalid GitHub repo "${ownerRepo}". Expected "owner/name".`
    )
  }
  return { owner: m[1]!, repo: m[2]! }
}

function authedRemoteUrl(owner: string, repo: string, token: string): string {
  // The "x-access-token" literal is GitHub's recommended username for
  // token-based HTTPS access.
  return `https://x-access-token:${token}@github.com/${owner}/${repo}.git`
}

async function run(
  args: { cmd: string; args: string[]; cwd?: string; env?: Record<string, string> }
): Promise<{ code: number; stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const child = spawn(args.cmd, args.args, {
      cwd: args.cwd,
      env: { ...process.env, ...args.env },
      stdio: [`ignore`, `pipe`, `pipe`],
    })
    let stdout = ``
    let stderr = ``
    child.stdout.on(`data`, (d: Buffer) => (stdout += d.toString()))
    child.stderr.on(`data`, (d: Buffer) => (stderr += d.toString()))
    child.on(`error`, reject)
    child.on(`close`, (code) => resolve({ code: code ?? -1, stdout, stderr }))
  })
}

async function pathExists(p: string): Promise<boolean> {
  try {
    await stat(p)
    return true
  } catch {
    return false
  }
}

/**
 * Ensure a local clone of the given GitHub repo exists at the canonical
 * companion path, with the remote pointed at the token-authenticated URL
 * and the default branch fetched. Returns the path so the worktree
 * manager can branch off it.
 */
export async function ensureRepo(args: {
  ownerRepo: string
  defaultBranch: string
  token: string
  log: Logger
}): Promise<RepoHandle> {
  const { owner, repo } = parseOwnerRepo(args.ownerRepo)
  const repoPath = join(REPOS_ROOT, owner, repo)
  const remoteUrl = authedRemoteUrl(owner, repo, args.token)

  await mkdir(REPOS_ROOT, { recursive: true })

  const gitDirExists = await pathExists(join(repoPath, `.git`))
  if (!gitDirExists) {
    args.log.info({ ownerRepo: args.ownerRepo }, `cloning repo`)
    await mkdir(join(REPOS_ROOT, owner), { recursive: true })
    const result = await run({
      cmd: `git`,
      args: [`clone`, remoteUrl, repoPath],
    })
    if (result.code !== 0) {
      throw new Error(
        `git clone failed for ${args.ownerRepo}: ${result.stderr.trim()}`
      )
    }
  } else {
    // Refresh the embedded token in the remote URL (it rotates) and pull
    // the latest default branch.
    await run({
      cmd: `git`,
      args: [`remote`, `set-url`, `origin`, remoteUrl],
      cwd: repoPath,
    })
    const fetched = await run({
      cmd: `git`,
      args: [`fetch`, `origin`, args.defaultBranch],
      cwd: repoPath,
    })
    if (fetched.code !== 0) {
      throw new Error(
        `git fetch failed for ${args.ownerRepo}: ${fetched.stderr.trim()}`
      )
    }
  }

  return {
    repoPath,
    owner,
    repo,
    defaultBranch: args.defaultBranch,
  }
}

/**
 * Push a branch to origin using a token-authenticated URL. Always rewrites
 * the remote first so a previously-stored stale token can't block the
 * push.
 */
export async function pushBranchWithToken(args: {
  repoPath: string
  owner: string
  repo: string
  branch: string
  token: string
  log: Logger
}): Promise<void> {
  await run({
    cmd: `git`,
    args: [
      `remote`,
      `set-url`,
      `origin`,
      authedRemoteUrl(args.owner, args.repo, args.token),
    ],
    cwd: args.repoPath,
  })
  const result = await run({
    cmd: `git`,
    args: [`push`, `-u`, `origin`, args.branch],
    cwd: args.repoPath,
  })
  if (result.code !== 0) {
    throw new Error(`git push failed: ${result.stderr.trim()}`)
  }
  args.log.info({ branch: args.branch }, `branch pushed`)
}
