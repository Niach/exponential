import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { App } from "./App"
import { loadGlyphs } from "./glyphs"
import { startRunner } from "./page"

const runner = startRunner()
// The glyphs the export carries (glyphs.json), loaded while the runner boots.
const icons = await loadGlyphs()
createRoot(document.getElementById(`root`)!).render(
  <StrictMode>
    <App runner={runner} icons={icons} />
  </StrictMode>,
)

// Offline: the service worker precaches the shell and the bundle (a new version is a new export).
if (import.meta.env.PROD && `serviceWorker` in navigator) void navigator.serviceWorker.register(`./sw.js`)
