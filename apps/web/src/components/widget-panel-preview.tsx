import { useEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import widgetCss from "@exp/widget/widget.css?inline"
import { paletteFor, pickForeground, resolveThemeMode } from "@exp/widget/theme"
import type { ThemePreference } from "@exp/widget/theme"
import { cn } from "@/lib/utils"

// Whole-window preview of the feedback panel (EXP-435): the REAL widget
// stylesheet (exported by @exp/widget) rendered over a static replica of the
// panel's feedback view, inside a shadow root — widget.css relies on
// `:host { all: initial }` and bare `*` resets, so it must never leak into
// the app's light DOM. The replica is inert display-only markup; drift risk
// is limited to structure since every color/spacing token is the shipped CSS.

// Undo the fixed launcher positioning so the panel sits inline in the dialog.
const previewOverrides = `
.exp-panel {
  position: static;
  width: 100%;
  max-width: none;
  max-height: none;
}
`

export interface WidgetPanelPreviewLabel {
  id: string
  name: string
  color: string
}

export function WidgetPanelPreview({
  theme,
  accentColor,
  labels,
  collectEmail,
  emailRequired,
  collectName,
  customFieldLabels,
  className,
}: {
  theme: ThemePreference
  // Empty string = the theme default (matches the settings form state).
  accentColor?: string
  labels: WidgetPanelPreviewLabel[]
  collectEmail: boolean
  emailRequired: boolean
  collectName: boolean
  customFieldLabels: string[]
  className?: string
}) {
  const hostRef = useRef<HTMLDivElement>(null)
  const [container, setContainer] = useState<HTMLElement | null>(null)

  useEffect(() => {
    const host = hostRef.current
    if (!host || host.shadowRoot) return
    const root = host.attachShadow({ mode: `open` })
    const style = document.createElement(`style`)
    style.textContent = `${widgetCss}\n${previewOverrides}`
    root.appendChild(style)
    const mount = document.createElement(`div`)
    root.appendChild(mount)
    setContainer(mount)
  }, [])

  const mode = resolveThemeMode(theme)
  const palette = paletteFor(mode)
  const accent = accentColor || palette.defaultAccent

  const replica = (
    <div
      className="exp-root"
      style={
        {
          "--exp-font": palette.font,
          "--exp-background": palette.background,
          "--exp-card": palette.card,
          "--exp-secondary": palette.secondary,
          "--exp-foreground": palette.foreground,
          "--exp-muted-foreground": palette.mutedForeground,
          "--exp-border": palette.border,
          "--exp-input": palette.input,
          "--exp-destructive": palette.destructive,
          "--exp-success": palette.success,
          "--exp-radius": palette.radius,
          "--exp-accent": accent,
          "--exp-accent-foreground": pickForeground(accent),
        } as React.CSSProperties
      }
    >
      <div className="exp-panel" role="presentation">
        <div className="exp-header">
          <div className="exp-header-lead">
            <h2>Send feedback</h2>
          </div>
          <button className="exp-close" type="button" aria-hidden="true">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            >
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="exp-body">
          <div className="exp-shot">
            <div className="exp-shot-empty">
              <span>Attach a screenshot of this page.</span>
              <div className="exp-shot-empty-actions">
                <button type="button" className="exp-chip">
                  Take screenshot
                </button>
              </div>
            </div>
          </div>
          <div className="exp-field">
            <label>Title</label>
            <input
              className="exp-input"
              placeholder="Something's broken on this page…"
              readOnly
            />
          </div>
          <div className="exp-field">
            <label>Details</label>
            <textarea
              className="exp-textarea"
              placeholder="What happened? What did you expect?"
              readOnly
            />
          </div>
          {labels.length > 0 && (
            <div className="exp-field">
              <label>What is this about?</label>
              <div className="exp-labels">
                {labels.map((label, index) => (
                  <button
                    type="button"
                    key={label.id}
                    className="exp-label-chip"
                    aria-pressed={index === 0}
                  >
                    <span
                      className="exp-label-dot"
                      style={{ background: label.color }}
                    />
                    {label.name}
                  </button>
                ))}
              </div>
            </div>
          )}
          {customFieldLabels.map((fieldLabel) => (
            <div className="exp-field" key={fieldLabel}>
              <label>{fieldLabel}</label>
              <input className="exp-input" readOnly />
            </div>
          ))}
          {collectName && (
            <div className="exp-field">
              <label>Name (optional)</label>
              <input className="exp-input" placeholder="Your name" readOnly />
            </div>
          )}
          {collectEmail && (
            <div className="exp-field">
              <label>{emailRequired ? `Email` : `Email (optional)`}</label>
              <input
                className="exp-input"
                placeholder="you@example.com"
                readOnly
              />
            </div>
          )}
          <div className="exp-footer" style={{ padding: 0, border: `none` }}>
            <button type="button" className="exp-submit">
              Send feedback
            </button>
          </div>
        </div>
        <div className="exp-powered">
          <a>
            Powered by <strong>Exponential</strong>
          </a>
        </div>
      </div>
    </div>
  )

  return (
    <div ref={hostRef} inert className={cn(`w-full`, className)}>
      {container ? createPortal(replica, container) : null}
    </div>
  )
}
