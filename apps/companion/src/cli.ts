#!/usr/bin/env bun
import { Command } from "commander"
import { runDaemon } from "./daemon"
import { runSetup } from "./commands/setup"
import { runInstallService } from "./commands/install-service"
import { runStatus } from "./commands/status"
import { runLogs } from "./commands/logs"

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
  .action(async (opts: { server: string; setupToken: string }) => {
    await runSetup({ server: opts.server, setupToken: opts.setupToken })
  })

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

await program.parseAsync(process.argv)
