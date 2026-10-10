import demoSurface from "@exponential-at/ui/fixtures/demo-surface.json"
import type { ThemesDoc } from "../lib/catalog"
import themesDoc from "@exponential-at/ui/docs/themes.generated.json"
import type { PageProps } from "../lib/routes"
import type { Nested } from "../sdk/a2ui"
import { themeBackgrounds } from "../sdk/backgrounds"
import { ThemeGallery } from "../sdk/ThemeGallery"

/* Themes: the three built-ins painting the same live surface, then the
   token vocabulary every theme must define with each built-in's values
   (docs/themes.generated.json), then the way on: the builder and the guide.
   The two JSON files are imported directly (not through lib/catalog) so
   this chunk does not carry the component docs and specimens. */

const THEMES_DOC: ThemesDoc = themesDoc

type TokenRow = (typeof THEMES_DOC.tokens)[keyof typeof THEMES_DOC.tokens][number]
type Shadow = { x: number; y: number; blur: number; spread: number; color: string }[]

const GROUP_LABELS: Record<string, string> = {
  color: `Colour (per mode)`,
  spacing: `Spacing (px)`,
  radius: `Radius (px)`,
  "type.size": `Type size (px)`,
  "type.lineHeight": `Line height (px)`,
  "type.weight": `Font weight`,
  "type.family": `Font family`,
  control: `Control heights (px)`,
  shadow: `Shadow (per mode)`,
  opacity: `Opacity`,
  border: `Border width (px)`,
  motion: `Motion (ms)`,
  breakpoint: `Breakpoint (px)`,
  ease: `Easing`,
  density: `Density scale`,
  blur: `Backdrop blur (px)`,
}

const isModed = (v: unknown): v is { light: unknown; dark: unknown } => v !== null && typeof v === `object` && !Array.isArray(v) && `light` in v && `dark` in v
const shadowCss = (layers: Shadow) => (layers.length ? layers.map((l) => `${l.x}px ${l.y}px ${l.blur}px ${l.spread}px ${l.color}`).join(`, `) : `none`)

export default function ThemesPage(_: PageProps) {
  const themes = THEMES_DOC.themes
  const groups = Object.entries(THEMES_DOC.tokens) as [string, TokenRow[]][]
  return (
    <div className="shell sdk-themes">
      <header className="docs-hero">
        <div className="docs-hero-content">
          <h1>Themes</h1>
          <div className="docs-hero-cta">
            <a className="btn btn-primary" href="/themes/builder/">
              Open the theme builder
            </a>
            <a className="btn btn-ghost" href="/guides/themes/">
              Write your own theme
            </a>
          </div>
        </div>
      </header>

      <section className="sdk-section"><ThemeGallery themes={themes} node={demoSurface as unknown as Nested} backgrounds={themeBackgrounds(THEMES_DOC)} /></section>

      <section className="sdk-section" id="tokens">
        <h2>Tokens</h2>
        {groups.map(([group, rows]) => (
          <div key={group} className="sdk-token-group">
            <h3>
              {GROUP_LABELS[group] ?? group}
            </h3>
            <div className="sdk-table-wrap">
              <table className="sdk-table sdk-token-table">
                <thead>
                  <tr>
                    <th>Token</th>
                    {themes.map((t) => (
                      <th key={t.id}>{t.name}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.ref}>
                      <td>
                        <code>{row.ref}</code>
                      </td>
                      {themes.map((t) => (
                        <td key={t.id}>
                          <TokenValue group={group} value={(row.values as Record<string, unknown>)[t.id]} />
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        ))}
      </section>

    </div>
  )
}

function TokenValue({ group, value }: { group: string; value: unknown }) {
  if (value === undefined) return <span className="sdk-dim">–</span>
  if (isModed(value)) {
    return (
      <span className="sdk-moded">
        {([`light`, `dark`] as const).map((m) => {
          const v = value[m]
          if (group === `color` && typeof v === `string`)
            return (
              <span key={m} className="sdk-swatch-row" title={`${m}: ${v}`}>
                <span className={`sdk-swatch is-${m}`}>
                  <span style={{ background: v }} />
                </span>
                <code>{v}</code>
              </span>
            )
          if (Array.isArray(v))
            return (
              <span key={m} className="sdk-swatch-row" title={`${m}: ${shadowCss(v as Shadow)}`}>
                <span className={`sdk-shadow is-${m}`}>
                  <span style={{ boxShadow: shadowCss(v as Shadow) }} />
                </span>
                <code>{(v as Shadow).length ? `${(v as Shadow).length} layer${(v as Shadow).length > 1 ? `s` : ``}` : `none`}</code>
              </span>
            )
          return (
            <span key={m} className="sdk-swatch-row">
              <code>
                {m}: {JSON.stringify(v)}
              </code>
            </span>
          )
        })}
      </span>
    )
  }
  return <code>{typeof value === `string` ? value : JSON.stringify(value)}</code>
}
