// #ERR-13: a tab opened before a deploy still runs the old build, whose
// route chunks (`/assets/<route>-<hash>.js`) no longer exist on the server.
// The router preloads routes on viewport, so the first sign is a failed
// background preload; a click on such a link then fails the navigation with
// "Failed to fetch dynamically imported module".
//
// The standard recovery: a failed NAVIGATION reloads the page once, at the
// route it was heading to; a failed background PRELOAD only marks the build
// stale (reloading under someone typing would lose their draft), and the
// next navigation becomes a full page load of its target. Both reloads share
// one sessionStorage guard so a chunk that is genuinely broken on the new
// build cannot loop.

/** sessionStorage key holding the epoch ms of the last recovery reload. */
export const STALE_BUILD_RELOAD_KEY = `exp:stale-build-reload`
/** A second recovery reload inside this window is refused (loop guard). */
export const STALE_BUILD_RELOAD_GUARD_MS = 30_000

const CHUNK_ERROR_PATTERNS = [
  // Chromium
  `Failed to fetch dynamically imported module`,
  // Firefox
  `error loading dynamically imported module`,
  // Safari
  `Importing a module script failed`,
  // Vite's preload helper (a route's CSS)
  `Unable to preload CSS for`,
]

/** True for the errors a missing (deleted) build chunk produces. */
export function isChunkLoadError(error: unknown): boolean {
  if (!error || typeof error !== `object`) return false
  const { name, message } = error as { name?: unknown; message?: unknown }
  if (name === `ChunkLoadError`) return true
  if (typeof message !== `string`) return false
  return CHUNK_ERROR_PATTERNS.some((p) => message.includes(p))
}

export type StaleBuildEnv = {
  storage: Pick<Storage, `getItem` | `setItem`> | null
  now: () => number
  /** A full page load of `href`. */
  load: (href: string) => void
}

function browserEnv(): StaleBuildEnv {
  let storage: StaleBuildEnv[`storage`] = null
  try {
    storage = window.sessionStorage
  } catch {
    storage = null
  }
  return {
    storage,
    now: () => Date.now(),
    load: (href) => {
      if (href === window.location.href) window.location.reload()
      else window.location.assign(href)
    },
  }
}

/**
 * Reload (a full page load of `href`) unless one already happened within the
 * guard window. Without sessionStorage there is no guard, so no reload.
 * Returns whether it reloaded.
 */
export function reloadForStaleBuild(
  href: string,
  env: StaleBuildEnv = browserEnv()
): boolean {
  const { storage, now } = env
  if (!storage) return false
  try {
    const last = Number(storage.getItem(STALE_BUILD_RELOAD_KEY) ?? 0)
    const at = now()
    if (Number.isFinite(last) && at - last < STALE_BUILD_RELOAD_GUARD_MS) {
      return false
    }
    storage.setItem(STALE_BUILD_RELOAD_KEY, String(at))
  } catch {
    return false
  }
  env.load(href)
  return true
}

let stale = false

/** Whether a background chunk load failed (the build is gone). */
export function isBuildStale(): boolean {
  return stale
}

/** A background load failed: the next navigation becomes a full load. */
export function markBuildStale(error: unknown): boolean {
  if (!isChunkLoadError(error)) return false
  stale = true
  return true
}

/**
 * The router's error boundary: a chunk-load error reloads the page at the
 * current URL (the navigation already committed it, so the route survives).
 * Returns whether it handled the error (a reload is on its way).
 */
export function recoverFromChunkError(
  error: unknown,
  env?: StaleBuildEnv
): boolean {
  if (typeof window === `undefined` || !isChunkLoadError(error)) return false
  stale = true
  return reloadForStaleBuild(window.location.href, env)
}

type NavigatingRouter = {
  subscribe: (
    event: `onBeforeNavigate`,
    fn: (e: { toLocation: { href: string } }) => void
  ) => () => void
}

/**
 * Wire the detectors once on the client: Vite's `vite:preloadError` and any
 * unhandled chunk-load rejection mark the build stale (the error still
 * propagates; nothing is swallowed), and the router's next navigation then
 * loads its target from the server. Returns the teardown.
 */
export function installStaleBuildRecovery(
  router: NavigatingRouter,
  env?: StaleBuildEnv
): () => void {
  if (typeof window === `undefined`) return () => {}
  const onPreloadError = (e: Event) =>
    void markBuildStale((e as Event & { payload?: unknown }).payload)
  const onRejection = (e: PromiseRejectionEvent) => void markBuildStale(e.reason)
  window.addEventListener(`vite:preloadError`, onPreloadError)
  window.addEventListener(`unhandledrejection`, onRejection)
  const unsubscribe = router.subscribe(`onBeforeNavigate`, (e) => {
    if (!stale) return
    const href = new URL(e.toLocation.href, window.location.origin).href
    reloadForStaleBuild(href, env)
  })
  return () => {
    window.removeEventListener(`vite:preloadError`, onPreloadError)
    window.removeEventListener(`unhandledrejection`, onRejection)
    unsubscribe()
  }
}

export function resetStaleBuildForTest(): void {
  stale = false
}
