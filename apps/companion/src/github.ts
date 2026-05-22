import { spawn } from "node:child_process"
import type { Logger } from "./logger"

interface OpenPRArgs {
  repoPath: string
  branch: string
  identifier: string
  title: string
  body: string
  draft?: boolean
}

export interface OpenPRResult {
  url: string
}

export async function openPullRequest(
  args: OpenPRArgs,
  log: Logger
): Promise<OpenPRResult> {
  const prTitle = `[${args.identifier}] ${args.title}`
  const ghArgs = [
    `pr`,
    `create`,
    `--title`,
    prTitle,
    `--body`,
    args.body,
    `--head`,
    args.branch,
  ]
  if (args.draft) ghArgs.push(`--draft`)
  return new Promise<OpenPRResult>((resolve, reject) => {
    const child = spawn(`gh`, ghArgs, { cwd: args.repoPath })
    let stdout = ``
    let stderr = ``
    child.stdout.on(`data`, (d: Buffer) => (stdout += d.toString()))
    child.stderr.on(`data`, (d: Buffer) => (stderr += d.toString()))
    child.on(`error`, (err) =>
      reject(new Error(`gh pr create failed to spawn: ${err.message}`))
    )
    child.on(`close`, (code) => {
      if (code !== 0) {
        return reject(
          new Error(`gh pr create exited ${code}: ${stderr.trim()}`)
        )
      }
      const url = stdout.trim().split(`\n`).pop() ?? ``
      if (!url.startsWith(`https://`)) {
        return reject(new Error(`gh pr create returned no URL: ${stdout}`))
      }
      log.info({ url }, `pr opened`)
      resolve({ url })
    })
  })
}

export async function pushBranch(
  repoPath: string,
  branch: string,
  log: Logger
): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const child = spawn(`git`, [`push`, `-u`, `origin`, branch], {
      cwd: repoPath,
    })
    let stderr = ``
    child.stderr.on(`data`, (d: Buffer) => (stderr += d.toString()))
    child.on(`error`, (err) => reject(err))
    child.on(`close`, (code) => {
      if (code !== 0) {
        return reject(new Error(`git push exited ${code}: ${stderr.trim()}`))
      }
      log.info({ branch }, `branch pushed`)
      resolve()
    })
  })
}

interface RunCommandArgs {
  cwd: string
  command: string
  signal?: AbortSignal
  timeoutMs?: number
}

export interface RunCommandResult {
  exitCode: number
  stdout: string
  stderr: string
}

export async function runCommand(
  args: RunCommandArgs
): Promise<RunCommandResult> {
  return new Promise<RunCommandResult>((resolve, reject) => {
    const child = spawn(`sh`, [`-c`, args.command], {
      cwd: args.cwd,
      signal: args.signal,
    })
    let stdout = ``
    let stderr = ``
    let timer: ReturnType<typeof setTimeout> | undefined
    if (args.timeoutMs) {
      timer = setTimeout(() => child.kill(`SIGTERM`), args.timeoutMs)
    }
    child.stdout.on(`data`, (d: Buffer) => (stdout += d.toString()))
    child.stderr.on(`data`, (d: Buffer) => (stderr += d.toString()))
    child.on(`error`, (err) => {
      if (timer) clearTimeout(timer)
      reject(err)
    })
    child.on(`close`, (code) => {
      if (timer) clearTimeout(timer)
      resolve({ exitCode: code ?? -1, stdout, stderr })
    })
  })
}
