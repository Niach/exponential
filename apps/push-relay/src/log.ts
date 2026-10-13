// EXP-1289: every relay log line carries the time it was written. The
// container's log driver keeps no timestamps of its own, so without this a
// burst of "FCM multicast failed" lines could not be placed in time at all
// (the 2026-10-13 investigation had to read the secret-gated /stats to learn
// the last failure was six days old).

export function formatLogLine(now: Date, message: string): string {
  return `[push-relay] ${now.toISOString()} ${message}`
}

export interface Logger {
  info(message: string): void
  warn(message: string): void
  error(message: string, err?: unknown): void
}

export const consoleLogger: Logger = {
  info(message) {
    console.log(formatLogLine(new Date(), message))
  },
  warn(message) {
    console.warn(formatLogLine(new Date(), message))
  },
  error(message, err) {
    if (err === undefined) console.error(formatLogLine(new Date(), message))
    else console.error(formatLogLine(new Date(), message), err)
  },
}
