/* The built-in themes side by side: the SAME surface (the home demo,
   specimen exponential-ui-demo) painted live in each, one light/dark switch
   for all. The cards prerender; the surfaces mount after hydration. */
import { useMemo, useState } from "react"
import type { ThemesDoc } from "../lib/catalog"
import { flatten, surfaceMessages, type Nested } from "./a2ui"
import type { Backgrounds } from "./backgrounds"
import { SurfacePlaceholder, useRuntime } from "./Island"

type Mode = `light` | `dark`

export function ThemeGallery({ themes, node, backgrounds }: { themes: ThemesDoc[`themes`]; node: Nested; backgrounds: Backgrounds }) {
  const [mode, setMode] = useState<Mode>(`dark`)
  const runtime = useRuntime()
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
        <span className="sdk-note">One surface, three theme files, no code change.</span>
      </div>
      <div className="sdk-gallery-grid">
        {themes.map((t) => (
          <figure key={t.id} className="sdk-gallery-card">
            <figcaption>
              <strong>{t.name}</strong>
              <code>{t.chain.join(` → `)}</code>
            </figcaption>
            <div className={`sdk-stage is-${mode} is-gallery`} style={{ background: backgrounds[t.id]?.[mode] }}>
              {runtime ? <runtime.LiveSurface surfaceId="demo" domId={`theme-${t.id}`} messages={messages} theme={t.id} mode={mode} icons={runtime.icons} /> : <SurfacePlaceholder minHeight={520} />}
            </div>
          </figure>
        ))}
      </div>
    </div>
  )
}
