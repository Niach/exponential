import { useEffect, useRef } from "react"
import type { PageProps } from "../lib/routes"

/* The theme builder (packages/exponential-ui/builder) mounted full width
   under the site header. The markup is the standalone page's body
   (builder/index.html) as JSX, so it prerenders; the builder's module and
   its scoped CSS (`.xb`) load in an effect and wire it up. Its preview is
   the real React renderer. */

const mounted = new WeakSet<HTMLElement>()

export default function ThemeBuilderPage(_: PageProps) {
  const root = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = root.current
    if (!el || mounted.has(el)) return
    mounted.add(el)
    void Promise.all([import("../../../../packages/exponential-ui/builder/mount"), import("../../../../packages/exponential-ui/builder/builder.css")]).then(([m]) => {
      m.mountBuilder(el, el)
      el.dataset.ready = `true`
    })
  }, [])
  return (
    <div className="sdk-builder-page">
      <div className="shell sdk-builder-intro">
        <h1>Theme builder</h1>
        <p>
          Edit tokens and recipes over a live preview; export the smallest <code>extends</code> theme. Starts from a base, a shadcn <code>globals.css</code>, a tweakcn export or a theme
          JSON. <a href="/guides/themes/">Theme guide</a>
        </p>
      </div>
      <div ref={root} className="xb sdk-builder" data-mode="dark">
        <header className="bar">
          <strong>Theme</strong>
          <label>
            Base <select id="base" />
          </label>
          <span className="seg">
            <button type="button" data-set-mode="light">
              Light
            </button>
            <button type="button" data-set-mode="dark">
              Dark
            </button>
          </span>
          <label>
            Width{` `}
            <select id="width" defaultValue="900">
              <option value="390">390 · phone</option>
              <option value="600">600</option>
              <option value="900">900</option>
              <option value="1200">1200</option>
            </select>
          </label>
          <span className="grow" />
          <button type="button" id="do-import">
            Import CSS / JSON…
          </button>
          <button type="button" id="reset">
            Reset
          </button>
          <button type="button" id="do-export" className="primary">
            Export JSON
          </button>
        </header>
        <main className="layout">
          <aside id="panel" className="panel">
            <p className="sdk-builder-loading">Loading the builder…</p>
          </aside>
          <section className="stage">
            <div id="issues" className="issues" hidden />
            <textarea id="export" className="mono export" rows={12} hidden readOnly />
            <div id="preview" className="preview" />
          </section>
        </main>
        <dialog id="import" className="import">
          <h2>Import</h2>
          <p>
            Paste a shadcn <code>globals.css</code> (v4 oklch or v3 HSL triplets), a tweakcn export, or an Exponential UI theme JSON. Colours, the radius ladder, fonts and shadows become
            overrides on the current base.
          </p>
          <textarea id="import-text" className="mono" rows={14} spellCheck={false} placeholder={`:root {\n  --background: oklch(1 0 0);\n  --primary: oklch(0.55 0.2 260);\n  --radius: 0.625rem;\n}\n.dark { … }`} />
          <pre id="import-report" className="mono" />
          <div className="actions">
            <button type="button" id="import-close">
              Close
            </button>
            <button type="button" id="import-apply" className="primary">
              Apply
            </button>
          </div>
        </dialog>
      </div>
    </div>
  )
}
