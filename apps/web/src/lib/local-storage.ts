// Guarded localStorage access shared by the per-device persistence modules
// (`last-visited.ts`, `coding-launch-prefs.ts`). The app is fully
// client-rendered (`defaultSsr: false`), but module code can still run where
// `window` is missing and localStorage access can throw (privacy modes,
// blocked storage) — callers degrade to "no persistence" on `null`.

export function safeLocalStorage(): Storage | null {
  if (typeof window === `undefined`) return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}
