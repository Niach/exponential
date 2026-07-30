import { createRouter as createTanstackRouter } from "@tanstack/react-router"

// Import the generated route tree
import { routeTree } from "./routeTree.gen"
import { getFirstTouch } from "@/lib/conversion/first-touch"

import "./styles.css"

// Create a new router instance
export function getRouter() {
  // Capture first-touch ref/utm params EAGERLY, before any beforeLoad
  // redirect (e.g. / → /onboarding) rewrites the URL (EXP-362; in-memory
  // only — cookieless by design).
  getFirstTouch()
  return createTanstackRouter({
    routeTree,
    defaultPreload: `viewport`,
    scrollRestoration: true,
  })
}
