import { afterEach, describe, expect, it, vi } from "vitest"
import {
  STALE_BUILD_RELOAD_GUARD_MS,
  STALE_BUILD_RELOAD_KEY,
  installStaleBuildRecovery,
  isBuildStale,
  isChunkLoadError,
  markBuildStale,
  recoverFromChunkError,
  reloadForStaleBuild,
  resetStaleBuildForTest,
  type StaleBuildEnv,
} from "./stale-build"

const CHROME = new TypeError(
  `Failed to fetch dynamically imported module: https://app.exponential.at/assets/general-abc123.js`
)

function env(start = 1_000_000) {
  const map = new Map<string, string>()
  let t = start
  const loads: string[] = []
  const e: StaleBuildEnv = {
    storage: {
      getItem: (k) => map.get(k) ?? null,
      setItem: (k, v) => void map.set(k, v),
    },
    now: () => t,
    load: (href) => void loads.push(href),
  }
  return { e, loads, map, advance: (ms: number) => (t += ms) }
}

function fakeRouter() {
  let listener: ((e: { toLocation: { href: string } }) => void) | null = null
  return {
    subscribe: vi.fn((_event: `onBeforeNavigate`, fn: typeof listener) => {
      listener = fn
      return () => (listener = null)
    }),
    navigate: (href: string) => listener?.({ toLocation: { href } }),
  }
}

afterEach(() => resetStaleBuildForTest())

describe(`isChunkLoadError`, () => {
  it.each([
    CHROME,
    new TypeError(`error loading dynamically imported module: /assets/mcp-x.js`),
    new TypeError(`Importing a module script failed.`),
    new Error(`Unable to preload CSS for /assets/general-x.css`),
    Object.assign(new Error(`Loading chunk 7 failed`), { name: `ChunkLoadError` }),
  ])(`recognises %s`, (error) => {
    expect(isChunkLoadError(error)).toBe(true)
  })

  it.each([new Error(`boom`), `Failed to fetch dynamically imported module`, null, {}])(
    `ignores %s`,
    (error) => {
      expect(isChunkLoadError(error)).toBe(false)
    }
  )
})

describe(`reloadForStaleBuild`, () => {
  it(`reloads once, then refuses inside the guard window`, () => {
    const { e, loads, map, advance } = env()
    expect(reloadForStaleBuild(`/a`, e)).toBe(true)
    expect(map.get(STALE_BUILD_RELOAD_KEY)).toBe(`1000000`)
    advance(STALE_BUILD_RELOAD_GUARD_MS - 1)
    expect(reloadForStaleBuild(`/a`, e)).toBe(false)
    expect(loads).toEqual([`/a`])
  })

  it(`allows a new recovery after the window (the next deploy)`, () => {
    const { e, loads, advance } = env()
    reloadForStaleBuild(`/a`, e)
    advance(STALE_BUILD_RELOAD_GUARD_MS)
    expect(reloadForStaleBuild(`/b`, e)).toBe(true)
    expect(loads).toEqual([`/a`, `/b`])
  })

  it(`never reloads without sessionStorage (no guard, no loop)`, () => {
    const { e, loads } = env()
    expect(reloadForStaleBuild(`/a`, { ...e, storage: null })).toBe(false)
    expect(loads).toEqual([])
  })

  it(`never reloads when sessionStorage throws`, () => {
    const { e, loads } = env()
    const storage = {
      getItem: () => {
        throw new Error(`denied`)
      },
      setItem: () => {},
    }
    expect(reloadForStaleBuild(`/a`, { ...e, storage })).toBe(false)
    expect(loads).toEqual([])
  })
})

describe(`recoverFromChunkError (the router's error boundary)`, () => {
  it(`reloads at the current URL, preserving the route`, () => {
    const { e, loads } = env()
    window.history.pushState({}, ``, `/t/acme/boards/general/issues/EXP-1`)
    expect(recoverFromChunkError(CHROME, e)).toBe(true)
    expect(loads).toEqual([`http://localhost:3000/t/acme/boards/general/issues/EXP-1`])
  })

  it(`leaves other errors to the reporter`, () => {
    const { e, loads } = env()
    expect(recoverFromChunkError(new Error(`boom`), e)).toBe(false)
    expect(loads).toEqual([])
  })

  it(`does not loop: a second chunk error right after the reload is reported`, () => {
    const { e, loads } = env()
    expect(recoverFromChunkError(CHROME, e)).toBe(true)
    expect(recoverFromChunkError(CHROME, e)).toBe(false)
    expect(loads).toHaveLength(1)
  })
})

describe(`installStaleBuildRecovery`, () => {
  it(`a failed preload marks the build stale; the next navigation is a full load of its target`, () => {
    const { e, loads } = env()
    const router = fakeRouter()
    const teardown = installStaleBuildRecovery(router, e)

    router.navigate(`/inbox`)
    expect(loads).toEqual([])

    const event = Object.assign(new Event(`vite:preloadError`, { cancelable: true }), {
      payload: CHROME,
    })
    window.dispatchEvent(event)
    expect(isBuildStale()).toBe(true)
    // Nothing is swallowed: Vite still rethrows to the router.
    expect(event.defaultPrevented).toBe(false)
    expect(loads).toEqual([])

    router.navigate(`/t/acme/boards/general?view=list`)
    expect(loads).toEqual([`http://localhost:3000/t/acme/boards/general?view=list`])
    teardown()
  })

  it(`an unrelated preload error does not mark the build stale`, () => {
    const { e, loads } = env()
    const router = fakeRouter()
    const teardown = installStaleBuildRecovery(router, e)
    window.dispatchEvent(
      Object.assign(new Event(`vite:preloadError`), { payload: new Error(`boom`) })
    )
    router.navigate(`/inbox`)
    expect(loads).toEqual([])
    teardown()
  })

  it(`teardown detaches the listeners`, () => {
    const router = fakeRouter()
    installStaleBuildRecovery(router, env().e)()
    window.dispatchEvent(Object.assign(new Event(`vite:preloadError`), { payload: CHROME }))
    expect(isBuildStale()).toBe(false)
  })

  it(`markBuildStale only accepts chunk errors`, () => {
    expect(markBuildStale(new Error(`boom`))).toBe(false)
    expect(markBuildStale(CHROME)).toBe(true)
  })
})
