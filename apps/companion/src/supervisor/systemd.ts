import { homedir } from "node:os"
import { join } from "node:path"
import { mkdir, writeFile } from "node:fs/promises"

export function systemdUnitPath(): string {
  return join(
    homedir(),
    `.config`,
    `systemd`,
    `user`,
    `exponential-companion.service`
  )
}

export async function writeSystemdUnit(): Promise<string> {
  const bunPath = process.execPath
  const cliPath = new URL(`../cli.ts`, import.meta.url).pathname

  const unit = `[Unit]
Description=Exponential Agent Companion
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${bunPath} ${cliPath} start
Restart=always
RestartSec=5
Environment=NODE_ENV=production

[Install]
WantedBy=default.target
`
  const path = systemdUnitPath()
  await mkdir(join(homedir(), `.config`, `systemd`, `user`), { recursive: true })
  await writeFile(path, unit, { mode: 0o644 })
  return path
}
