const CACHE_NAME = `exponential-v1`
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

  // Skip API routes entirely
  if (url.pathname.startsWith(`/api/`)) return

  // Cache-first for static assets
  if (STATIC_ASSETS.some((asset) => url.pathname === asset) || url.pathname.match(/\.(woff2?|ttf|otf|css|js|png|svg|ico)$/)) {
    event.respondWith(caches.match(request).then((cached) => cached || fetch(request).then((response) => {
      const clone = response.clone()
      caches.open(CACHE_NAME).then((cache) => cache.put(request, clone))
      return response
    })))
    return
  }

  // Network-first for everything else
  event.respondWith(fetch(request).catch(() => caches.match(request)))
})
