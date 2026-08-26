const CACHE_NAME = `exponential-v3`
const STATIC_ASSETS = [`/icon-192.png`, `/icon-512.png`, `/apple-touch-icon.png`, `/logo-dark.svg`, `/logo-light.svg`]

self.addEventListener(`install`, (event) => {
  event.waitUntil(caches.open(CACHE_NAME).then((cache) => cache.addAll(STATIC_ASSETS)))
  self.skipWaiting()
})

self.addEventListener(`activate`, (event) => {
  event.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k))))
  )
  self.clients.claim()
})

self.addEventListener(`fetch`, (event) => {
  const { request } = event
  const url = new URL(request.url)

  // Only handle same-origin requests
  if (url.origin !== self.location.origin) return

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
