/* ─── WidgetPanelDemo — faithful static recreation of the REAL feedback
   widget panel (packages/widget/src/ui/Panel.tsx + widget.css + theme.ts).
   Copy, layout, sizes and colors are transcribed 1:1 from the widget source
   (380px card panel, 12/16 header, 13.5px inputs, near-white submit,
   "Powered by Exponential" footer, megaphone FAB). The real widget lives in
   a shadow root under exp-* classes; this recreation uses cw-* (collab.css)
   so the two can never collide. Decorative only — rendered inside an inert,
   aria-hidden stage. Plays the ONE form (EXP-602, SLOP-4): captured
   screenshot, the message, filed as an issue. */

import { WIDGET_FILED_ISSUE } from "../webui/data"

const svgProps = {
  viewBox: `0 0 24 24`,
  fill: `none`,
  stroke: `currentColor`,
  strokeLinecap: `round`,
  strokeLinejoin: `round`,
} as const

export function MegaphoneIcon({ size = 16 }: { size?: number }) {
  return (
    <svg {...svgProps} width={size} height={size} strokeWidth={2} aria-hidden>
      <path d="m3 11 18-5v12L3 14v-3z" />
      <path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" />
    </svg>
  )
}

function CloseIcon() {
  return (
    <svg {...svgProps} width={14} height={14} strokeWidth={2} aria-hidden>
      <path d="M18 6 6 18M6 6l12 12" />
    </svg>
  )
}

function CheckIcon() {
  return (
    <svg {...svgProps} width={18} height={18} strokeWidth={2.5} aria-hidden>
      <path d="M20 6 9 17l-5-5" />
    </svg>
  )
}

export type WidgetDemoView = `form` | `success`

export function WidgetPanelDemo({
  view,
  message,
  emailFilled,
  caret,
}: {
  view: WidgetDemoView
  /* The one form field (the scene types it in). */
  message: string
  /* The email field fills once the message is in. */
  emailFilled: boolean
  /* Blinking caret in the textarea while typing. */
  caret: boolean
}) {
  return (
    <div className={`cw-panel`}>
      {view === `success` ? (
        <div className={`cw-success`}>
          <span className={`cw-success-icon`}>
            <CheckIcon />
          </span>
          <span className={`cw-success-title`}>Thanks, your report is in.</span>
          <span className={`cw-success-sub`}>
            Filed as{` `}
            <span className={`cw-success-link`}>{WIDGET_FILED_ISSUE.id}</span>.
            {` `}We emailed you a link to follow the conversation.
          </span>
        </div>
      ) : (
        <>
          <div className={`cw-header`}>
            <span className={`cw-header-lead`}>
              <span className={`cw-title`}>Send feedback</span>
            </span>
            <span className={`cw-close`}>
              <CloseIcon />
            </span>
          </div>

        <div className={`cw-body`}>
            {/* Captured screenshot preview + action chips (the real form's
                post-capture state; the thumbnail is a page silhouette). */}
            <div className={`cw-shot`}>
              <div className={`cw-shot-img`}>
                <span className={`cw-shot-bar is-w60`} />
                <span className={`cw-shot-bar is-w80`} />
                <span className={`cw-shot-bar is-w40`} />
              </div>
              <div className={`cw-shot-actions`}>
                <span className={`cw-chip`}>Annotate</span>
                <span className={`cw-chip`}>Retake</span>
                <span className={`cw-chip`}>Remove</span>
              </div>
            </div>
            <div className={`cw-field`}>
              <span className={`cw-label`}>What happened?</span>
              <div className={`cw-textarea`}>
                {message.length === 0 && (
                  <span className={`cw-placeholder`}>
                    Describe what you saw, or what you&apos;d like to see.
                  </span>
                )}
                {message}
                {caret && <span className={`cw-caret`} />}
              </div>
            </div>
            <div className={`cw-field`}>
              <span className={`cw-label`}>Email (optional)</span>
              <div className={`cw-input`}>
                {emailFilled ? (
                  `mara@heliolabs.io`
                ) : (
                  <span className={`cw-placeholder`}>you@example.com</span>
                )}
              </div>
            </div>
            <div className={`cw-footer`}>
              <span className={`cw-submit`}>Send feedback</span>
            </div>
          </div>
        </>
      )}
      <div className={`cw-powered`}>
        Powered by <strong>Exponential</strong>
      </div>
    </div>
  )
}
