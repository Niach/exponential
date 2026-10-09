import type { PageProps } from "../lib/routes"
import PlaygroundApp from "../sdk/PlaygroundApp"

/* The playground. The whole app prerenders (the editor holds the first
   example), so the largest paint is in the HTML; the renderer, the icons
   and the share codec load in effects after hydration. */
export default function PlaygroundPage(_: PageProps) {
  return (
    <div className="shell sdk-playground-page">
      <header className="docs-hero sdk-playground-hero">
        <div className="docs-hero-content">
          <span className="section-eyebrow">Playground</span>
          <h1>Paste A2UI, see it render</h1>
          <p>Messages (JSON or JSONL), a flat component list or a nested node, rendered live in any theme. Share it as a link; read the prompt a model gets.</p>
        </div>
      </header>
      <PlaygroundApp />
    </div>
  )
}
