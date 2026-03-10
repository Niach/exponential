import * as React from "react"
import {
  Outlet,
  HeadContent,
  Scripts,
  createRootRoute,
} from "@tanstack/react-router"
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools"
import { TooltipProvider } from "@/components/ui/tooltip"

import "../styles.css?url"

export const Route = createRootRoute({
  head: () => ({
    meta: [
      {
        charSet: `utf-8`,
      },
      {
        name: `viewport`,
        content: `width=device-width, initial-scale=1`,
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
        content: `Real-time issue tracker`,
      },
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
      {
        rel: `icon`,
        type: `image/svg+xml`,
        href: `/logo-dark.svg`,
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
    <TooltipProvider>
      <Outlet />
      {showRouterDevtools && <TanStackRouterDevtools />}
    </TooltipProvider>
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
