import * as React from "react"
import {
  Outlet,
  HeadContent,
  Scripts,
  createRootRoute,
} from "@tanstack/react-router"
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools"
import { Toaster, TooltipProvider } from "@exp/ui"
import { SessionProvider } from "@/hooks/use-session"
import { APP_TITLE, appUrl } from "@/lib/page-title"

import "../styles.css?url"

export const Route = createRootRoute({
  head: () => ({
    meta: [
      {
        charSet: `utf-8`,
      },
      {
        name: `viewport`,
        content: `width=device-width, initial-scale=1, viewport-fit=cover`,
      },
      {
        title: `Exponential`,
      },
      {
        name: `theme-color`,
        content: `#09090b`,
      },
      {
        name: `description`,
        content: `Issues, support and coding agents in one realtime workspace`,
      },
      {
        // No page of the app instance is indexable — public feedback boards
        // included (EXP-99). server-bun.ts pairs this with an X-Robots-Tag
        // header; the marketing site is the indexable surface.
        name: `robots`,
        content: `noindex`,
      },
      // FEED-88: link unfurlers (Linear, Slack) fetch app URLs anonymously
      // and read only the SSR shell, so the card names the product, never
      // synced data (nothing is anonymously readable). Routes override
      // `og:title` from their params alone.
      { property: `og:site_name`, content: APP_TITLE },
      { property: `og:type`, content: `website` },
      { property: `og:title`, content: APP_TITLE },
      {
        property: `og:description`,
        content: `Issues, support and coding agents in one realtime workspace`,
      },
      { property: `og:image`, content: appUrl(`/og-card.png`) },
      { property: `og:image:width`, content: `1200` },
      { property: `og:image:height`, content: `630` },
      { name: `twitter:card`, content: `summary_large_image` },
      { name: `twitter:image`, content: appUrl(`/og-card.png`) },
      {
        name: `apple-mobile-web-app-capable`,
        content: `yes`,
      },
      {
        name: `apple-mobile-web-app-status-bar-style`,
        content: `black-translucent`,
      },
    ],
    links: [
      {
        rel: `preconnect`,
        href: `https://fonts.googleapis.com`,
      },
      {
        rel: `preconnect`,
        href: `https://fonts.gstatic.com`,
        crossOrigin: `anonymous`,
      },
      {
        rel: `stylesheet`,
        href: `https://fonts.googleapis.com/css2?family=Inter:wght@300..700&display=swap`,
      },
      {
        rel: `manifest`,
        href: `/manifest.json`,
      },
      // Raster first: unfurlers that skip SVG favicons take the first PNG.
      {
        rel: `icon`,
        type: `image/png`,
        sizes: `192x192`,
        href: `/icon-192.png`,
      },
      {
        rel: `icon`,
        type: `image/svg+xml`,
        href: `/logo-light.svg`,
      },
      {
        rel: `icon`,
        sizes: `any`,
        href: `/favicon.ico`,
      },
      {
        rel: `apple-touch-icon`,
        href: `/apple-touch-icon.png`,
      },
    ],
  }),
  shellComponent: RootDocument,
  component: RootComponent,
})

function RootComponent() {
  const disableRouterDevtools =
    import.meta.env.VITE_DISABLE_ROUTER_DEVTOOLS === `1` ||
    (typeof navigator !== `undefined` && navigator.webdriver) ||
    (typeof process !== `undefined` &&
      process.env?.DISABLE_ROUTER_DEVTOOLS === `1`)
  const showRouterDevtools = !disableRouterDevtools

  React.useEffect(() => {
    if (`serviceWorker` in navigator) {
      navigator.serviceWorker.register(`/sw.js`)
    }
  }, [])

  return (
    <SessionProvider>
      <TooltipProvider>
        {/* The Toaster comes BEFORE the Outlet: effects run in tree order,
            and sonner's Toaster only subscribes in an effect and never
            replays earlier toasts, so it must subscribe before any route
            effect can toast (a first-render toast is otherwise lost).
            Gated by src/root-toaster-order.test.tsx. */}
        <Toaster />
        <Outlet />
        {showRouterDevtools && <TanStackRouterDevtools />}
      </TooltipProvider>
    </SessionProvider>
  )
}

function RootDocument({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <head>
        <HeadContent />
      </head>
      <body>
        {children}
        <Scripts />
      </body>
    </html>
  )
}
