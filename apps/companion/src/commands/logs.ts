import { homedir, platform } from "node:os"
import { join } from "node:path"
import { spawn } from "node:child_process"

export async function runLogs(): Promise<void> {
  const os = platform()
  if (os === `darwin`) {
    const logFile = join(
      homedir(),
      `Library`,
      `Logs`,
      `exponential-companion`,
      `daemon.log`
    )
    const child = spawn(`tail`, [`-F`, logFile], { stdio: `inherit` })
    child.on(`exit`, (code) => process.exit(code ?? 0))
    return
  }
  if (os === `linux`) {
    const child = spawn(
      `journalctl`,
      [`--user`, `-u`, `exponential-companion`, `-f`, `-n`, `200`],
      { stdio: `inherit` }
    )
    child.on(`exit`, (code) => process.exit(code ?? 0))
    return
  }
  console.error(`logs: unsupported platform ${os}`)
  process.exit(1)
}
