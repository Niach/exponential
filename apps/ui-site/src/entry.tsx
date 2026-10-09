import { StrictMode } from "react"
import { createRoot, hydrateRoot } from "react-dom/client"
import { App } from "./app"
import { normalizePath, pageLoaderFor } from "./lib/pages"
import "./styles.css"

/* The route's page module loads BEFORE hydration (each page type is its own
   chunk, so the home page never downloads the playground), then the
   prerendered markup hydrates; dev mounts fresh. */
void (async () => {
  const container = document.getElementById(`root`)
  if (!container) throw new Error(`root not found`)
  let path = normalizePath(location.pathname)
  let load = pageLoaderFor(path)
  if (!load) {
    path = `/`
    load = pageLoaderFor(path)!
  }
  const page = await load()
  const app = (
    <StrictMode>
      <App path={path} page={page} />
    </StrictMode>
  )
  if (container.firstChild) hydrateRoot(container, app)
  else createRoot(container).render(app)
})()
