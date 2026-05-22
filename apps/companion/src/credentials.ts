import { readFile, writeFile, mkdir } from "node:fs/promises"
import { join } from "node:path"
import { CONFIG_DIR } from "./config"

// MVP credentials store: a 0600 file in the companion config dir.
const TOKEN_FILE = join(CONFIG_DIR, `bot.token`)

export async function writeBotToken(token: string): Promise<void> {
  await mkdir(CONFIG_DIR, { recursive: true })
  await writeFile(TOKEN_FILE, token + `\n`, { mode: 0o600 })
}

export async function readBotToken(): Promise<string> {
  const raw = await readFile(TOKEN_FILE, `utf-8`)
  const trimmed = raw.trim()
  if (!trimmed.startsWith(`expk_`)) {
    throw new Error(
      `Bot token at ${TOKEN_FILE} is empty or missing the expk_ prefix. Re-run \`companion init\`.`
    )
  }
  return trimmed
}
