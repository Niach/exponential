import { useEffect, useState, type ComponentType } from "react"
import type { PageProps } from "../lib/routes"

/* The playground. The page prerenders its intro and a sized placeholder;
   the editor, the renderer, the prompt generator and the share codec load
   as one client chunk after hydration (src/sdk/PlaygroundApp.tsx). */
export default function PlaygroundPage(_: PageProps) {
  const [App, setApp] = useState<ComponentType | null>(null)
  useEffect(() => {
    let live = true
    void import("../sdk/PlaygroundApp").then((m) => live && setApp(() => m.default))
    return () => {
      live = false
    }
  }, [])
  return (
    <div className="shell sdk-playground-page">
      <header className="docs-hero sdk-playground-hero">
        <div className="docs-hero-content">
          <span className="section-eyebrow">Playground</span>
          <h1>Paste A2UI, see it render</h1>
          <p>
            A2UI v0.9 messages (a JSON array or JSONL), a flat component list or a nested node, rendered live by the React renderer in any theme. Share it as a link; read the
            system prompt a model gets for the same catalog.
          </p>
        </div>
      </header>
      {App ? (
        <App />
      ) : (
        <div className="sdk-placeholder sdk-playground-placeholder" aria-busy="true">
          <span>Loading the playground…</span>
        </div>
      )}
    </div>
  )
}
