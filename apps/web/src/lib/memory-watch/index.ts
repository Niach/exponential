// #EXP-2133: the memory watch. Every minute it reads the host's available
// memory and swap and the containers' usage against their limits; a condition
// that holds for three readings is an alert: a log line, an event in
// diagnostics (GlitchTip) and one mail per MEMORY_ALERT_EMAIL address. The mail
// does not depend on GlitchTip, which may be what eats the memory.
//
// Off unless MEMORY_WATCH=on: exactly one web container per host needs it.
import { captureMessage } from "@sentry/core"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { DEFAULT_THRESHOLDS, MemoryAlertState, type MemoryAlert, type MemoryThresholds } from "./evaluate"
import { readMemory, type MemoryReading } from "./readings"

const READ_INTERVAL_MS = 60 * 1000

type Env = Record<string, string | undefined>

const shareOf = (raw: string | undefined, fallback: number): number => {
  const n = Number(raw)
  return raw?.trim() && Number.isFinite(n) && n >= 0 && n <= 100 ? n : fallback
}

export function memoryWatchEnabled(env: Env): boolean {
  const flag = (env.MEMORY_WATCH ?? ``).trim().toLowerCase()
  return flag === `on` || flag === `true` || flag === `1`
}

export function thresholdsFrom(env: Env): MemoryThresholds {
  return {
    hostAvailablePercent: shareOf(env.MEMORY_WATCH_HOST_AVAILABLE_PERCENT, DEFAULT_THRESHOLDS.hostAvailablePercent),
    swapFreePercent: shareOf(env.MEMORY_WATCH_SWAP_FREE_PERCENT, DEFAULT_THRESHOLDS.swapFreePercent),
    containerUsedPercent: shareOf(env.MEMORY_WATCH_CONTAINER_USED_PERCENT, DEFAULT_THRESHOLDS.containerUsedPercent),
  }
}

export function alertRecipients(env: Env): string[] {
  return (env.MEMORY_ALERT_EMAIL ?? ``)
    .split(`,`)
    .map((address) => address.trim())
    .filter((address) => address.includes(`@`))
}

export interface MemoryWatchDeps {
  env: Env
  read: () => Promise<MemoryReading>
  report: (alert: MemoryAlert) => void
  mail: (to: string, subject: string, text: string) => Promise<unknown>
  now: () => number
}

async function mailAlert(to: string, subject: string, text: string): Promise<void> {
  // Lazy: the mail transport (aws-sdk) stays out of the boot graph.
  const { sendEmail } = await import(`@/lib/email`)
  const html = `<p>${text.replace(/&/g, `&amp;`).replace(/</g, `&lt;`).replace(/\n/g, `<br>`)}</p>`
  const result = await sendEmail({ to, subject, text, html })
  if (!result.delivered) throw new Error(result.suppressed ? `recipient suppressed` : `not delivered (no mail transport?)`)
}

/** One watch: call `tick` every minute. A failed mail never stops the others. */
export function createMemoryWatch(overrides: Partial<MemoryWatchDeps> = {}): { tick: () => Promise<MemoryAlert[]>; reading: () => MemoryReading | null } {
  const env = overrides.env ?? process.env
  const deps: MemoryWatchDeps = {
    env,
    read: () => readMemory({ cgroupRoot: env.MEMORY_WATCH_CGROUP_ROOT?.trim() || undefined }),
    report: (alert) =>
      void captureMessage(alert.text, { level: `error`, fingerprint: [`memory-watch`, alert.key], tags: { alert: alert.key } }),
    mail: mailAlert,
    now: () => Date.now(),
    ...overrides,
  }
  const state = new MemoryAlertState(thresholdsFrom(env))
  let last: MemoryReading | null = null
  return {
    reading: () => last,
    tick: async () => {
      last = await deps.read()
      const alerts = state.next(last, deps.now())
      if (alerts.length === 0) return alerts
      for (const alert of alerts) {
        console.error(`[memory-watch] ${alert.text}`)
        deps.report(alert)
      }
      const host = env.BETTER_AUTH_URL ? ` (${env.BETTER_AUTH_URL})` : ``
      const text = `${alerts.map((alert) => `${alert.text}.`).join(`\n`)}\n\nReported by the memory watch${host}. Container names are the ids docker ps shows.`
      for (const to of alertRecipients(env)) {
        try {
          await deps.mail(to, `Memory alert: ${alerts[0]!.text}`, text)
        } catch (err) {
          console.error(`[memory-watch] mail failed:`, err)
        }
      }
      return alerts
    },
  }
}

const mb = (bytes: number) => Math.round(bytes / 1048576)

let started = false

/** Starts the watch; call once at boot (server-bun.ts). */
export function startMemoryWatch(): void {
  if (started || !memoryWatchEnabled(process.env)) return
  started = true
  const watch = createMemoryWatch()
  let running = false
  const run = async () => {
    if (running) return
    running = true
    const startMs = performance.now()
    try {
      const alerts = await watch.tick()
      const reading = watch.reading()
      reportSchedulerRun(`memory-watch`, {
        ok: true,
        durationMs: performance.now() - startMs,
        detail: `${reading?.host ? `${mb(reading.host.availableBytes)} MB available, ${mb(reading.host.swapFreeBytes)} MB swap free` : `no host reading`}, ${reading?.containers.length ?? 0} container(s), ${alerts.length} alert(s)`,
      })
    } catch (err) {
      reportSchedulerRun(`memory-watch`, { ok: false, durationMs: performance.now() - startMs, error: String(err) })
      console.error(`[memory-watch] reading failed:`, err)
    } finally {
      running = false
    }
  }
  setInterval(() => void run(), READ_INTERVAL_MS)
  void run()
}
