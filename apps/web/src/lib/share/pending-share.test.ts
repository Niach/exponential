// @vitest-environment node
// EXP-1635: the pending share: a pure replay of what share-targets.json fixes
// for the web, the reader against what `public/sw.js` really writes (the
// worker runs here in a vm), and the manifest's POST wired to the worker.
import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { runInNewContext } from "node:vm"
import { beforeEach, describe, expect, it, vi } from "vitest"
import manifest from "../../../public/manifest.json"
import { TEMPORARY_PICK } from "@exp/domain-contract/composer-agent-row"
import {
  SHARE_CACHE_NAME,
  clearPendingShare,
  holdPendingShare,
  loadPendingShare,
  shareChatSeed,
  shareImages,
  shareIsEmpty,
  shareIssuePrefill,
  type PendingShare,
} from "@/lib/share/pending-share"

const share = (over: Partial<PendingShare> = {}): PendingShare => ({
  title: ``,
  text: ``,
  url: ``,
  files: [],
  ...over,
})
const png = (name = `a.png`) => new File([`pixels`], name, { type: `image/png` })
const pdf = () => new File([`doc`], `a.pdf`, { type: `application/pdf` })

/** The Cache API, keyed by pathname, as the browser's. */
function fakeCaches() {
  const stores = new Map<string, Map<string, Response>>()
  const key = (request: string | Request) =>
    new URL(typeof request === `string` ? request : request.url, `https://app.test/`).pathname
  return {
    stores,
    open: async (name: string) => {
      const store = stores.get(name) ?? new Map<string, Response>()
      stores.set(name, store)
      return {
        put: async (request: string | Request, response: Response) => void store.set(key(request), response),
        match: async (request: string | Request) => store.get(key(request))?.clone(),
        addAll: async () => {},
      }
    },
    delete: async (name: string) => stores.delete(name),
    keys: async () => [...stores.keys()],
    match: async () => undefined,
  }
}

/** Loads the real worker; returns what it registered. */
function loadWorker(storage: ReturnType<typeof fakeCaches>) {
  const handlers = new Map<string, (event: unknown) => void>()
  const self = {
    addEventListener: (name: string, handler: (event: unknown) => void) => handlers.set(name, handler),
    location: new URL(`https://app.test/sw.js`),
    skipWaiting: () => {},
    clients: { claim: () => {} },
  }
  const source = readFileSync(resolve(import.meta.dirname, `../../../public/sw.js`), `utf8`)
  runInNewContext(source, { self, caches: storage, URL, Response, fetch: vi.fn(), Promise, console })
  return handlers
}

const dispatchFetch = (handlers: Map<string, (event: unknown) => void>, request: Request) => {
  let responded: Promise<Response> | undefined
  handlers.get(`fetch`)!({
    request,
    respondWith: (response: Promise<Response>) => {
      responded = response
    },
  })
  return responded
}

const postShare = (fields: Record<string, string>, files: File[] = []) => {
  const form = new FormData()
  for (const [name, value] of Object.entries(fields)) form.set(name, value)
  for (const file of files) form.append(`files`, file)
  return new Request(new URL(manifest.share_target.action, `https://app.test`), {
    method: manifest.share_target.method,
    body: form,
  })
}

describe(`shareChatSeed`, () => {
  it(`carries the fixture's text, the files and the pick; nothing else`, () => {
    const files = [png()]
    expect(shareChatSeed(share({ title: `IMG_1.png`, text: ` look `, files }), TEMPORARY_PICK)).toEqual({
      issueIds: [],
      text: `look`,
      files,
      pick: TEMPORARY_PICK,
    })
  })

  it(`a link's page title leads a text share`, () => {
    expect(shareChatSeed(share({ title: `Crash report`, url: `https://x.test/a` }), `a1`).text).toBe(
      `Crash report\n\nhttps://x.test/a`
    )
  })

  it(`a file's title is a filename, images or not`, () => {
    expect(shareChatSeed(share({ title: `a.pdf`, files: [pdf()] }), `a1`).text).toBeUndefined()
  })
})

describe(`shareIssuePrefill (the natives' mapping)`, () => {
  it(`title = the first line, description = the rest`, () => {
    expect(shareIssuePrefill(share({ text: `Crash\nstack trace`, url: `https://x.test` }))).toEqual({
      title: `Crash`,
      description: `stack trace\n\nhttps://x.test`,
    })
  })

  it(`a single line leaves no description`, () => {
    expect(shareIssuePrefill(share({ text: `Crash` }))).toEqual({ title: `Crash`, description: `` })
  })

  it(`the subject titles a share with no text`, () => {
    expect(shareIssuePrefill(share({ title: `Crash report` }))).toEqual({ title: `Crash report`, description: `` })
  })

  it(`never a filename`, () => {
    expect(shareIssuePrefill(share({ title: `IMG_1.png`, files: [png()] }))).toEqual({ title: ``, description: `` })
  })

  it(`caps the title at 120`, () => {
    expect(shareIssuePrefill(share({ text: `x`.repeat(200) })).title).toHaveLength(120)
  })
})

describe(`shareIsEmpty + shareImages`, () => {
  it(`blank fields and no files = nothing shared`, () => {
    expect(shareIsEmpty(share({ text: `  ` }))).toBe(true)
    expect(shareIsEmpty(share({ files: [pdf()] }))).toBe(false)
  })

  it(`only images preview`, () => {
    const image = png()
    expect(shareImages(share({ files: [image, pdf()] }))).toEqual([image])
  })
})

describe(`the worker's share_target (public/sw.js)`, () => {
  let storage: ReturnType<typeof fakeCaches>

  beforeEach(async () => {
    storage = fakeCaches()
    vi.stubGlobal(`caches`, storage)
    await clearPendingShare()
  })

  it(`the manifest posts multipart title, text, url and files to the worker's path`, () => {
    expect(manifest.share_target).toMatchObject({
      action: `/api/share-target`,
      method: `POST`,
      enctype: `multipart/form-data`,
      params: { title: `title`, text: `text`, url: `url` },
    })
    expect(manifest.share_target.params.files[0]).toEqual({ name: `files`, accept: [`image/*`, `*/*`] })
  })

  it(`stashes the POST, 303s to /share, and the page reads back fields and files`, async () => {
    const handlers = loadWorker(storage)
    const response = await dispatchFetch(
      handlers,
      postShare({ title: `T`, text: `body`, url: `https://x.test` }, [png(), pdf()])
    )
    expect(response?.status).toBe(303)
    expect(response?.headers.get(`location`)).toBe(`https://app.test/share`)

    const pending = await loadPendingShare()
    expect(pending).toMatchObject({ title: `T`, text: `body`, url: `https://x.test` })
    expect(pending?.files.map(({ name, type }) => ({ name, type }))).toEqual([
      { name: `a.png`, type: `image/png` },
      { name: `a.pdf`, type: `application/pdf` },
    ])
    expect(await pending?.files[0]?.text()).toBe(`pixels`)
  })

  it(`a new share replaces the pending one`, async () => {
    const handlers = loadWorker(storage)
    await dispatchFetch(handlers, postShare({ text: `first` }, [png(), png(`b.png`)]))
    await dispatchFetch(handlers, postShare({ text: `second` }))
    expect(await loadPendingShare()).toEqual({ title: ``, text: `second`, url: ``, files: [] })
  })

  it(`an unreadable POST still lands on /share with nothing pending`, async () => {
    const handlers = loadWorker(storage)
    const bad = new Request(`https://app.test/api/share-target`, { method: `POST`, body: `not a form` })
    expect((await dispatchFetch(handlers, bad))?.status).toBe(303)
    expect(await loadPendingShare()).toBeNull()
  })

  it(`activating a new worker keeps the pending share, drops old caches`, async () => {
    const handlers = loadWorker(storage)
    await dispatchFetch(handlers, postShare({ text: `kept` }))
    await storage.open(`exponential-v1`)
    let done: Promise<unknown> = Promise.resolve()
    handlers.get(`activate`)!({
      waitUntil: (work: Promise<unknown>) => {
        done = work
      },
    })
    await done
    expect(storage.stores.has(SHARE_CACHE_NAME)).toBe(true)
    expect(storage.stores.has(`exponential-v1`)).toBe(false)
  })

  it(`every other request keeps today's handling`, () => {
    const handlers = loadWorker(storage)
    expect(dispatchFetch(handlers, new Request(`https://app.test/api/trpc/x`, { method: `POST`, body: `{}` }))).toBeUndefined()
    expect(dispatchFetch(handlers, new Request(`https://other.test/api/share-target`, { method: `POST`, body: `x` }))).toBeUndefined()
  })
})

describe(`pending share lifecycle`, () => {
  beforeEach(async () => {
    vi.stubGlobal(`caches`, fakeCaches())
    await clearPendingShare()
  })

  it(`stays pending across reads until cleared`, async () => {
    holdPendingShare(share({ text: `x` }))
    expect(await loadPendingShare()).toMatchObject({ text: `x` })
    expect(await loadPendingShare()).toMatchObject({ text: `x` })
    await clearPendingShare()
    expect(await loadPendingShare()).toBeNull()
  })

  it(`a browser with no Cache API has nothing pending`, async () => {
    vi.stubGlobal(`caches`, undefined)
    expect(await loadPendingShare()).toBeNull()
  })
})
