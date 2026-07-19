/* ─── WidgetPanelDemo — faithful static recreation of the REAL feedback
   widget panel (packages/widget/src/ui/Panel.tsx + widget.css + theme.ts).
   Copy, layout, sizes and colors are transcribed 1:1 from the widget source
   (380px card panel, 12/16 header, mode cards, 13.5px inputs, near-white
   submit, "Powered by Exponential" footer, megaphone FAB). The real widget
   lives in a shadow root under exp-* classes; this recreation uses cw-*
   (collab.css) so the two can never collide. Decorative only — rendered
   inside an inert, aria-hidden stage. */

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

function BackIcon() {
  return (
    <svg {...svgProps} width={15} height={15} strokeWidth={2} aria-hidden>
      <path d="m15 18-6-6 6-6" />
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

export type WidgetDemoView = `home` | `support` | `success`

export function WidgetPanelDemo({
  view,
  message,
  emailFilled,
  caret,
}: {
  view: WidgetDemoView
  /* Support-form message body (the scene types it in). */
  message: string
  /* The email field fills once the message is done. */
  emailFilled: boolean
  /* Blinking caret in the message box while typing. */
  caret: boolean
}) {
  return (
    <div className={`cw-panel`}>
      {view === `success` ? (
        <div className={`cw-success`}>
          <span className={`cw-success-icon`}>
            <CheckIcon />
          </span>
          <span className={`cw-success-title`}>We got your request!</span>
          <span className={`cw-success-sub`}>
            Check your email &mdash; we sent you a link to track the
            conversation and reply.
          </span>
        </div>
      ) : (
        <>
          <div className={`cw-header`}>
            <span className={`cw-header-lead`}>
              {view === `support` && (
                <span className={`cw-back`}>
                  <BackIcon />
                </span>
              )}
              <span className={`cw-title`}>
                {view === `home` ? `Hi there 👋` : `Get help`}
              </span>
            </span>
            <span className={`cw-close`}>
              <CloseIcon />
            </span>
          </div>

          {view === `home` ? (
            <div className={`cw-body`}>
              <span className={`cw-home-sub`}>How can we help?</span>
              <div className={`cw-mode-card`}>
                <span className={`cw-mode-title`}>Give feedback</span>
                <span className={`cw-mode-sub`}>
                  Report a bug or share an idea &mdash; screenshot included.
                </span>
              </div>
              <div className={`cw-mode-card is-picked`}>
                <span className={`cw-mode-title`}>Get help</span>
                <span className={`cw-mode-sub`}>
                  Ask us anything &mdash; we&apos;ll reply by email.
                </span>
              </div>
            </div>
          ) : (
            <div className={`cw-body`}>
              <div className={`cw-field`}>
                <span className={`cw-label`}>How can we help?</span>
                <div className={`cw-textarea`}>
                  {message.length === 0 && (
                    <span className={`cw-placeholder`}>
                      Describe your question or problem&hellip;
                    </span>
                  )}
                  {message}
                  {caret && <span className={`cw-caret`} />}
                </div>
              </div>
              <div className={`cw-field`}>
                <span className={`cw-label`}>Email</span>
                <div className={`cw-input`}>
                  {emailFilled ? (
                    `mara@heliolabs.io`
                  ) : (
                    <span className={`cw-placeholder`}>you@example.com</span>
                  )}
                </div>
              </div>
              <div className={`cw-footer`}>
                <span className={`cw-submit`}>Send request</span>
              </div>
            </div>
          )}
        </>
      )}
      <div className={`cw-powered`}>
        Powered by <strong>Exponential</strong>
      </div>
    </div>
  )
}
