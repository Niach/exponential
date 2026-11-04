const CACHE_NAME = `exponential-v3`
const STATIC_ASSETS = [`/icon-192.png`, `/icon-512.png`, `/apple-touch-icon.png`, `/logo-dark.svg`, `/logo-light.svg`]

// EXP-1635: the manifest's `share_target` POST. The one pending share (fields
// + files) waits in its OWN cache, `lib/share/pending-share.ts` is the reader,
// until `/share` hands it on. Its cache outlives a CACHE_NAME bump: a share
// must survive a worker update. A browser with no worker reaches the server's
// fallback (`routes/api/share-target.ts`) instead.
const SHARE_CACHE_NAME = `exponential-share`
const SHARE_TARGET_PATH = `/api/share-target`
const SHARE_PAGE_PATH = `/share`
const SHARE_META_KEY = `/__share/meta`
const shareFileKey = (index) => `/__share/file/${index}`

const shareField = (form, name) => {
  const value = form.get(name)
  return typeof value === `string` ? value : ``
}

async function stashShare(request) {
  try {
    const form = await request.formData()
    const files = form.getAll(`files`).filter((file) => typeof file !== `string` && file.size > 0)
    // Single pending share: a new one replaces whatever was waiting.
    await caches.delete(SHARE_CACHE_NAME)
    const cache = await caches.open(SHARE_CACHE_NAME)
    await Promise.all(files.map((file, index) => cache.put(shareFileKey(index), new Response(file))))
    await cache.put(
      SHARE_META_KEY,
      Response.json({
        title: shareField(form, `title`),
        text: shareField(form, `text`),
        url: shareField(form, `url`),
        files: files.map(({ name, type, lastModified }) => ({ name, type, lastModified })),
      })
    )
  } catch (error) {
    // Nothing stashed: `/share` finds no pending share and goes home.
    console.warn(`share target`, error)
  }
  return Response.redirect(new URL(SHARE_PAGE_PATH, self.location.origin).href, 303)
}

self.addEventListener(`install`, (event) => {
  event.waitUntil(caches.open(CACHE_NAME).then((cache) => cache.addAll(STATIC_ASSETS)))
  self.skipWaiting()
})

self.addEventListener(`activate`, (event) => {
  event.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE_NAME && k !== SHARE_CACHE_NAME).map((k) => caches.delete(k))))
  )
  self.clients.claim()
})

self.addEventListener(`fetch`, (event) => {
  const { request } = event
  const url = new URL(request.url)

  // Only handle same-origin requests
  if (url.origin !== self.location.origin) return

  if (request.method === `POST` && url.pathname === SHARE_TARGET_PATH) {
    event.respondWith(stashShare(request))
    return
  }

  // Skip API routes entirely — and server-function RPCs with them (EXP-632):
  // they are POST-shaped data calls whose responses must never be replayed
  // from a cache, and a cached HTML shell answered for one would break the
  // page far more quietly than a failed request.
  if (url.pathname.startsWith(`/api/`) || url.pathname.startsWith(`/_serverFn/`)) return

  // Cache-first for explicitly listed static assets only (icons, logos)
  if (STATIC_ASSETS.some((asset) => url.pathname === asset)) {
    event.respondWith(caches.match(request).then((cached) => cached || fetch(request).then((response) => {
      if (response.ok) {
        const clone = response.clone()
        caches.open(CACHE_NAME).then((cache) => cache.put(request, clone))
      }
      return response
    })))
    return
  }

  // Network-first for everything else (HTML, hashed build assets, etc.).
  // `caches.match` resolves UNDEFINED on a miss, and respondWith(undefined)
  // throws — the offline path has to end in a Response either way.
  event.respondWith(
    fetch(request).catch(async () => (await caches.match(request)) ?? Response.error())
  )
})
