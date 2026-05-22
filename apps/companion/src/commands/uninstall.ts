import { spawn } from "node:child_process"
import { rm, unlink } from "node:fs/promises"
import { CONFIG_DIR, STATE_DIR } from "../config"
import { systemdUnitPath } from "../supervisor/systemd"
import { uninstallSelf } from "../exponential-api"
import { loadConfig } from "../config"

interface Opts {
  keepState: boolean
  keepAgent: boolean
}

function runQuiet(
  cmd: string,
  args: string[]
): Promise<{ code: number; output: string }> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, { stdio: [`ignore`, `pipe`, `pipe`] })
    let output = ``
    child.stdout.on(`data`, (d: Buffer) => (output += d.toString()))
    child.stderr.on(`data`, (d: Buffer) => (output += d.toString()))
    child.on(`error`, () => resolve({ code: -1, output }))
    child.on(`close`, (code) => resolve({ code: code ?? -1, output }))
  })
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await unlink(path)
    return true
  } catch {
    // If we couldn't unlink, fall back to a soft "doesn't exist" — rm with
    // force handles either case below.
    return false
  }
}

export async function runUninstall(opts: Opts): Promise<void> {
  console.log(`Stopping exponential-companion service…`)
  await runQuiet(`systemctl`, [
    `--user`,
    `disable`,
    `--now`,
    `exponential-companion`,
  ])

  console.log(`Removing systemd user unit…`)
  const unitPath = systemdUnitPath()
  await pathExists(unitPath)
  await runQuiet(`systemctl`, [`--user`, `daemon-reload`])

  if (!opts.keepAgent) {
    console.log(`Revoking agent on the server…`)
    try {
      const config = await loadConfig()
      await uninstallSelf(config)
    } catch (e) {
      console.error(
        `  Server revoke failed (continuing with local cleanup): ${e instanceof Error ? e.message : e}`
      )
    }
  } else {
    console.log(`Skipping server revoke (--keep-agent).`)
  }

  console.log(`Wiping ${CONFIG_DIR}`)
  await rm(CONFIG_DIR, { recursive: true, force: true })

  if (!opts.keepState) {
    console.log(`Wiping ${STATE_DIR}`)
    await rm(STATE_DIR, { recursive: true, force: true })
  } else {
    console.log(`Skipping local state wipe (--keep-state).`)
  }

  console.log(``)
  console.log(`Companion uninstalled.`)
}
