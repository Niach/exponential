import { useSyncExternalStore } from "react"
import { safeLocalStorage } from "@/lib/local-storage"

// #LOGS-2: "Send diagnostics" (Settings › Account › Privacy), a per-DEVICE
// preference: one localStorage key, never synced, default ON. Off = the
// client SDK is never initialised (or is closed at once when switched off).
// Dependency-free of Sentry: the settings page reads it without loading the SDK.

export const SEND_DIAGNOSTICS_STORAGE_KEY = `exp.sendDiagnostics`

const listeners = new Set<() => void>()
// The last choice made on this page: wins when storage is blocked.
let chosen: boolean | null = null

export function readSendDiagnostics(): boolean {
  if (chosen !== null) return chosen
  try {
    return safeLocalStorage()?.getItem(SEND_DIAGNOSTICS_STORAGE_KEY) !== `off`
  } catch {
    return true
  }
}

export function writeSendDiagnostics(on: boolean): void {
  chosen = on
  const storage = safeLocalStorage()
  try {
    if (on) storage?.removeItem(SEND_DIAGNOSTICS_STORAGE_KEY)
    else storage?.setItem(SEND_DIAGNOSTICS_STORAGE_KEY, `off`)
  } catch {
    /* blocked storage: the choice lasts for this page only */
  }
  for (const listener of listeners) listener()
}

/** Tests only. */
export function resetSendDiagnosticsForTest(): void {
  chosen = null
}

export function subscribeSendDiagnostics(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function useSendDiagnostics(): [boolean, (on: boolean) => void] {
  const on = useSyncExternalStore(
    subscribeSendDiagnostics,
    readSendDiagnostics,
    () => true
  )
  return [on, writeSendDiagnostics]
}
