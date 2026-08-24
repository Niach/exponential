/**
 * Child-process helpers shared by the capture lanes (EXP-566).
 *
 * A capture run spawns a lot of other people's programs — docker, fastlane,
 * gradle, xcodebuild, screencapture, a second bun. Two behaviours matter enough
 * to centralise:
 *
 *   - STREAMED output. A fastlane lane runs for ten minutes; swallowing its
 *     stdout until it exits makes a live run indistinguishable from a hang.
 *   - Nothing outlives the orchestrator. Long-lived children (the relay stub,
 *     the desktop app) are killed in a `finally` and on SIGINT/SIGTERM, so a
 *     Ctrl-C never leaves a stray desktop window or a relay socket behind.
 */

export interface RunResult {
  code: number
  stdout: string
  stderr: string
}

export interface RunOptions {
  cmd: string[]
  cwd?: string
  env?: Record<string, string | undefined>
  /** Mirror the child's output to this process as it arrives. */
  stream?: boolean
  /** Prefix for streamed lines, e.g. `[ios]`. */
  label?: string
  timeoutMs?: number
}

/** Run a command to completion, capturing (and optionally mirroring) its output. */
export async function run(options: RunOptions): Promise<RunResult> {
  const child = Bun.spawn({
    cmd: options.cmd,
    cwd: options.cwd,
    env: { ...process.env, ...options.env } as Record<string, string>,
    stdout: `pipe`,
    stderr: `pipe`,
    stdin: `ignore`,
  })

  let timer: ReturnType<typeof setTimeout> | undefined
  let timedOut = false
  if (options.timeoutMs) {
    timer = setTimeout(() => {
      timedOut = true
      child.kill()
    }, options.timeoutMs)
  }

  const [stdout, stderr] = await Promise.all([
    drain(child.stdout, options.stream ? process.stdout : undefined, options.label),
    drain(child.stderr, options.stream ? process.stderr : undefined, options.label),
  ])
  const code = await child.exited
  if (timer) clearTimeout(timer)

  return {
    code: timedOut ? 124 : code,
    stdout,
    stderr: timedOut ? `${stderr}\n[timed out after ${options.timeoutMs}ms]` : stderr,
  }
}

async function drain(
  stream: ReadableStream<Uint8Array> | number | undefined,
  mirror: NodeJS.WriteStream | undefined,
  label?: string
): Promise<string> {
  if (!stream || typeof stream === `number`) return ``
  const decoder = new TextDecoder()
  let text = ``
  let pending = ``
  for await (const chunk of stream as ReadableStream<Uint8Array>) {
    const piece = decoder.decode(chunk, { stream: true })
    text += piece
    if (!mirror) continue
    pending += piece
    const lines = pending.split(`\n`)
    pending = lines.pop() ?? ``
    for (const line of lines) mirror.write(label ? `${label} ${line}\n` : `${line}\n`)
  }
  if (mirror && pending) mirror.write(label ? `${label} ${pending}\n` : `${pending}\n`)
  return text
}

/** Is this executable on PATH? */
export async function hasCommand(name: string): Promise<boolean> {
  const result = await run({ cmd: [`which`, name] })
  return result.code === 0
}

export type Child = ReturnType<typeof Bun.spawn>

const alive = new Set<Child>()
let handlersInstalled = false

/**
 * Register a long-lived child so it dies with us. Bun.spawn does not expose
 * `detached`, so children share our process group and a Ctrl-C at the terminal
 * already reaches them — but a programmatic SIGTERM (or `kill %1`) does not,
 * and neither does an exception path. Belt and braces: kill the pid, then try
 * the group, and swallow the ESRCH the second attempt usually raises.
 */
export function track(child: Child): Child {
  alive.add(child)
  if (!handlersInstalled) {
    handlersInstalled = true
    const stop = (signal: NodeJS.Signals) => {
      killAll()
      process.exit(signal === `SIGINT` ? 130 : 143)
    }
    process.on(`SIGINT`, () => stop(`SIGINT`))
    process.on(`SIGTERM`, () => stop(`SIGTERM`))
    process.on(`exit`, () => killAll())
  }
  return child
}

/** Kill one tracked child and its process group. */
export function killChild(child: Child): void {
  alive.delete(child)
  try {
    child.kill()
  } catch {
    /* already gone */
  }
  try {
    process.kill(-child.pid, `SIGKILL`)
  } catch {
    /* not a group leader, or already reaped */
  }
}

export function killAll(): void {
  for (const child of [...alive]) killChild(child)
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
