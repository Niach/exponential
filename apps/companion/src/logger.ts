import pino from "pino"
import type { CompanionConfig } from "./config"

export type Logger = pino.Logger

export function createLogger(_config: CompanionConfig): Logger {
  const level = process.env.LOG_LEVEL || `info`
  return pino({
    level,
    transport: process.stdout.isTTY
      ? { target: `pino-pretty`, options: { translateTime: `HH:MM:ss` } }
      : undefined,
  })
}
