/* The built-in themes side by side: the SAME surface (the home demo,
   specimen exponential-ui-demo) painted live in each, one light/dark switch
   for all. The cards prerender; the surfaces mount after hydration. */
import { useMemo, useRef } from "react"
import type { ThemesDoc } from "../lib/catalog"
import { useModePick } from "../lib/scheme"
import { flatten, surfaceMessages, type Nested } from "./a2ui"
import { groundVars, type Backgrounds } from "./backgrounds"
import { SurfacePlaceholder, useNearViewport, useRuntime } from "./Island"

export function ThemeGallery({ themes, node, backgrounds }: { themes: ThemesDoc[`themes`]; node: Nested; backgrounds: Backgrounds }) {
  const { mode, cls: modeClass, setMode } = useModePick()
  const grid = useRef<HTMLDivElement>(null)
  const runtime = useRuntime(useNearViewport(grid))
  const messages = useMemo(() => surfaceMessages(`demo`, flatten(node)), [node])
  return (
    <div className="sdk-gallery">
      <div className="sdk-studio-bar">
        <div className="sdk-seg" role="group" aria-label="Mode">
          {([`light`, `dark`] as const).map((m) => (
            <button key={m} type="button" aria-pressed={mode === m} onClick={() => setMode(m)}>
              {m === `light` ? `Light` : `Dark`}
            </button>
          ))}
        </div>
      </div>
      <div ref={grid} className="sdk-gallery-grid">
        {themes.map((t) => (
          <figure key={t.id} className="sdk-gallery-card">
            <figcaption>
              <strong>{t.name}</strong>
            </figcaption>
            <div className={`sdk-stage is-${modeClass} is-gallery`} style={groundVars(backgrounds[t.id])}>
              {runtime ? <runtime.LiveSurface surfaceId="demo" domId={`theme-${t.id}`} messages={messages} theme={t.id} mode={mode} icons={runtime.icons} /> : <SurfacePlaceholder minHeight={520} />}
            </div>
          </figure>
        ))}
      </div>
    </div>
  )
}
