// VAPP-269: the browser half of the web runner: the stores (SQLite-WASM on
// OPFS in a worker, IndexedDB when that cannot open, files in OPFS, the Vault
// in IndexedDB), the E5 web capabilities, and the page events that drive
// lifecycle.json (launch, visibility, pagehide, online, links, the md
// breakpoint). No network but the bundle's own files.

import { builtinTheme } from "@exponential_app/eui"
import type { LifecycleEvent } from "@exp/eengine"
import { browserPlatform, openWebStorage, sqlClient, webCapabilities, type OpfsDirectory, type SqlDatabase } from "@exp/eengine/web"
import runnerFixture from "@exp/eengine/fixtures/runner.json" with { type: "json" }

import { deepLinkOf, Runner } from "./runner"

/** genUI's `md` breakpoint: wide from it up, else compact. */
export const MD = (builtinTheme(`exponential`).tokens as { breakpoint?: { md?: number } }).breakpoint?.md ?? 768

/** Open the worker's SQLite on OPFS; rejects when it cannot (no sync access handles, another tab holds it). */
function openSqlite(appId: string): Promise<SqlDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof Worker === `undefined`) return reject(new Error(`no workers`))
    const worker = new Worker(new URL(`./sqlite-worker.ts`, import.meta.url), { type: `module` })
    const timer = setTimeout(() => (worker.terminate(), reject(new Error(`sqlite timed out`))), 10_000)
    const onReady = (event: MessageEvent) => {
      const data = event.data as { ready?: boolean; failed?: string }
      if (!data || (data.ready === undefined && data.failed === undefined)) return
      clearTimeout(timer)
      worker.removeEventListener(`message`, onReady)
      if (data.ready) resolve(sqlClient(worker))
      else {
        worker.terminate()
        reject(new Error(data.failed))
      }
    }
    worker.addEventListener(`message`, onReady)
    worker.addEventListener(`error`, (e) => (clearTimeout(timer), reject(new Error(e.message))))
    worker.postMessage({ open: appId })
  })
}

const opfsRoot = async (): Promise<OpfsDirectory> => {
  const root = (await navigator.storage.getDirectory()) as unknown as OpfsDirectory
  return root.getDirectoryHandle(runnerFixture.storage.files, { create: true })
}

/** The bundle dir `app/` beside the page (runner.json `bundle.locations.web`). */
const readBundle = async (name: string): Promise<string | null> => {
  const response = await fetch(new URL(`${runnerFixture.bundle.dir}/${name}`, document.baseURI), { cache: `no-cache` }).catch(() => null)
  return response?.ok ? response.text() : null
}

export function startRunner(): Runner {
  const wide = window.matchMedia(`(min-width: ${MD}px)`)
  let runner: Runner
  const capabilities = webCapabilities(browserPlatform(), { onTap: (tap) => void runner.event(tap as LifecycleEvent) })
  runner = new Runner({
    readBundle,
    openStorage: async (appId) => {
      const storage = await openWebStorage({ indexedDB, idbName: `exp-runner/${appId}`, openSqlite: () => openSqlite(appId), files: opfsRoot })
      void navigator.storage?.persist?.()
      // Which records backend opened (the capture script reads it; nothing leaves the page).
      ;(globalThis as { __runnerBackend?: string }).__runnerBackend = storage.backend
      return storage
    },
    capabilities,
    formFactor: wide.matches ? `wide` : `compact`,
    locale: navigator.language,
  })
  void runner.start(deepLinkOf(new URL(location.href))).then(() => {
    if (runner.config) document.title = runner.config.name
    wide.addEventListener(`change`, () => runner.setFormFactor(wide.matches ? `wide` : `compact`))
    document.addEventListener(`visibilitychange`, () => void runner.event({ type: document.visibilityState === `hidden` ? `background` : `foreground` }))
    // A page kept in the back/forward cache comes back; one that goes for good terminates.
    window.addEventListener(`pagehide`, (e: PageTransitionEvent) => void runner.event({ type: e.persisted ? `background` : `terminate` }))
    window.addEventListener(`pageshow`, (e: PageTransitionEvent) => {
      if (e.persisted) void runner.event({ type: `foreground` })
    })
    addEventListener(`online`, () => void runner.event({ type: `online` }))
    addEventListener(`hashchange`, () => {
      const link = deepLinkOf(new URL(location.href))
      if (link) void runner.event(link)
    })
  })
  return runner
}
