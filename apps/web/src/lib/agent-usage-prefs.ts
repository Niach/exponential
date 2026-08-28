// EXP-484: which usage window the reader pinned, per agent. A local reading
// habit, not server state — the device rewrites `devices.agent_usage` every
// few minutes and a pinned key that vanishes just falls back to the fullest
// window (`selectWindow`).
//
// Mirrored per client with the same key shape:
//   iOS      UI/Session/AgentUsageWindowPrefs.swift (UserDefaults)
//   Android  data/AgentUsageWindowPrefs.kt (SecureStore)
//   desktop  coding::Settings.usage_window (settings.json, never on the wire)
//
// Every accessor guards via `safeLocalStorage` and degrades to "no pin".

import { safeLocalStorage } from "@/lib/local-storage"

const KEY_PREFIX = `exp.agentUsageWindow.`

function storageKey(agent: string): string {
  return `${KEY_PREFIX}${agent}`
}

export function readAgentUsageWindow(agent: string): string | null {
  const store = safeLocalStorage()
  if (!store) return null
  try {
    const raw = store.getItem(storageKey(agent))
    return raw && raw.length > 0 ? raw : null
  } catch {
    return null
  }
}

export function writeAgentUsageWindow(agent: string, key: string): void {
  const store = safeLocalStorage()
  if (!store) return
  try {
    if (key.length === 0) {
      store.removeItem(storageKey(agent))
      return
    }
    store.setItem(storageKey(agent), key)
  } catch {
    // Quota/privacy failures just mean the pin resets next visit.
  }
}

export function clearAgentUsageWindow(agent: string): void {
  const store = safeLocalStorage()
  if (!store) return
  try {
    store.removeItem(storageKey(agent))
  } catch {
    // Nothing to recover from — the pin is a convenience.
  }
}
