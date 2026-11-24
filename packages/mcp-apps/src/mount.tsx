import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { App, type AppView, type AppViews } from "./app"
import "./styles.css"

/** #NEW-24: one document per view (`<view>.html`, vite.config.ts); its entry
 *  (src/entries/) mounts the shell with the view's own components. */
export function mount(view: AppView, views: AppViews): void {
  const root = document.getElementById(`root`)
  if (!root) return
  createRoot(root).render(
    <StrictMode>
      <App view={view} views={views} />
    </StrictMode>
  )
}
