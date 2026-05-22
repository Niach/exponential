#!/usr/bin/env bun
import { Command } from "commander"
import { runDaemon } from "./daemon"
import { runSetup } from "./commands/setup"
import { runInstallService } from "./commands/install-service"
import { runStatus } from "./commands/status"
import { runLogs } from "./commands/logs"
import { runUninstall } from "./commands/uninstall"

const program = new Command()
  .name(`companion`)
  .description(`Exponential agent companion daemon`)
  .version(`0.1.0`)

program
  .command(`setup`)
  .description(
    `Claim a web-issued setup token and configure this Linux companion.`
  )
  .requiredOption(`--server <url>`, `Exponential base URL.`)
  .requiredOption(
    `--setup-token <token>`,
    `One-time setup token from the web app.`
  )
  .option(
    `--driver <driver>`,
    `Coding driver: claude or codex (default claude).`,
    `claude`
  )
  .action(
    async (opts: {
      server: string
      setupToken: string
      driver: string
    }) => {
      if (opts.driver !== `claude` && opts.driver !== `codex`) {
        throw new Error(`--driver must be 'claude' or 'codex'`)
      }
      await runSetup({
        server: opts.server,
        setupToken: opts.setupToken,
        driver: opts.driver,
      })
    }
  )

program
  .command(`start`)
  .description(`Run the daemon in the foreground.`)
  .action(async () => {
    await runDaemon()
  })

program
  .command(`install-service`)
  .description(`Install as a systemd --user service on Linux.`)
  .action(async () => {
    await runInstallService()
  })

program
  .command(`status`)
  .description(`Show daemon health + in-flight issues.`)
  .action(async () => {
    await runStatus()
  })

program
  .command(`logs`)
  .description(`Tail the daemon log file.`)
  .action(async () => {
    await runLogs()
  })

program
  .command(`uninstall`)
  .description(
    `Stop and remove the companion: disables the systemd unit, revokes the agent on the server, wipes local state.`
  )
  .option(
    `--keep-state`,
    `Don't delete ~/.exponential-companion (sqlite state, worktrees, baileys-auth).`
  )
  .option(
    `--keep-agent`,
    `Don't revoke the agent on the server; leave the workspace_agents row in place.`
  )
  .action(async (opts: { keepState?: boolean; keepAgent?: boolean }) => {
    await runUninstall({
      keepState: opts.keepState ?? false,
      keepAgent: opts.keepAgent ?? false,
    })
  })

await program.parseAsync(process.argv)
