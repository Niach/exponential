import { useEffect, useRef, useState } from "preact/hooks"
import type { Screenshot } from "./App"

const closeIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12"/></svg>`

// Same growth loop as the public board's powered-by footer: every embedded
// widget signposts back to the product.
function PoweredBy() {
  return (
    <div className="exp-powered">
      <a
        href="https://exponential.at/?utm_source=widget"
        target="_blank"
        rel="noopener noreferrer"
      >
        Powered by <strong>Exponential</strong>
      </a>
    </div>
  )
}
const checkIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>`
const backIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m15 18-6-6 6-6"/></svg>`

// Which pane the panel shows: the card home (only when both modes are
// enabled) or one of the two forms directly.
export type PanelView = `home` | `feedback` | `support`

const viewTitles: Record<PanelView, string> = {
  home: `Hi there 👋`,
  feedback: `Send feedback`,
  support: `Get help`,
}

export function Panel(props: {
  phase: `open` | `submitting` | `success`
  // Mounted-but-invisible while the annotation editor is open, so the typed
  // form fields survive.
  hidden?: boolean
  view: PanelView
  // True when both modes exist (the home screen is reachable).
  canGoBack: boolean
  onPickMode(mode: `feedback` | `support`): void
  onBack(): void
  successFlavor: `feedback` | `support`
  successIdentifier: string | null
  // Absolute public issue URL — non-null only when the target project is a
  // public feedback board (older servers never send it).
  successUrl: string | null
  position: `bottom-right` | `bottom-left`
  screenshot: Screenshot | null
  // The annotated screenshot is still being encoded; sending now would
  // submit the un-annotated (uncropped) base image.
  flattening: boolean
  captureFailed: boolean
  identityEmail: string | null
  emailRequired: boolean
  onClose(): void
  // Capture from scratch (empty state) — lands in the annotator on success.
  onCapture(): void
  onRetake(): void
  onAnnotate(): void
  onRemoveScreenshot(): void
  onSubmit(form: {
    title: string
    description: string
    email: string
  }): Promise<string | null>
  onSubmitSupport(form: {
    message: string
    email: string
  }): Promise<string | null>
}) {
  const [title, setTitle] = useState(``)
  const [description, setDescription] = useState(``)
  const [email, setEmail] = useState(``)
  const [message, setMessage] = useState(``)
  const [supportEmail, setSupportEmail] = useState(``)
  const [error, setError] = useState<string | null>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const titleRef = useRef<HTMLInputElement>(null)
  const messageRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    setError(null)
    if (props.view === `feedback`) titleRef.current?.focus()
    if (props.view === `support`) messageRef.current?.focus()
  }, [props.view])

  // Escape closes; Tab cycles within the panel (shadow-root focus trap).
  useEffect(() => {
    const panel = panelRef.current
    if (!panel) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === `Escape`) {
        event.stopPropagation()
        props.onClose()
        return
      }
      if (event.key !== `Tab`) return
      const focusables = panel.querySelectorAll<HTMLElement>(
        `button, a[href], input, textarea, [tabindex]:not([tabindex="-1"])`
      )
      if (focusables.length === 0) return
      const first = focusables[0]
      const last = focusables[focusables.length - 1]
      const active = (panel.getRootNode() as ShadowRoot).activeElement
      if (event.shiftKey && active === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && active === last) {
        event.preventDefault()
        first.focus()
      }
    }
    panel.addEventListener(`keydown`, onKeyDown)
    return () => panel.removeEventListener(`keydown`, onKeyDown)
  }, [props.onClose])

  const submit = async (event: Event) => {
    event.preventDefault()
    if (props.phase === `submitting` || props.flattening) return
    setError(null)
    const trimmedTitle = title.trim()
    if (!trimmedTitle) {
      setError(`Add a short title for the report.`)
      titleRef.current?.focus()
      return
    }
    if (props.emailRequired && !props.identityEmail && !email.trim()) {
      setError(`Your email is required.`)
      return
    }
    const failure = await props.onSubmit({
      title: trimmedTitle,
      description: description.trim(),
      email: email.trim(),
    })
    if (failure) setError(failure)
  }

  const submitSupport = async (event: Event) => {
    event.preventDefault()
    if (props.phase === `submitting`) return
    setError(null)
    const trimmedMessage = message.trim()
    if (!trimmedMessage) {
      setError(`Tell us what you need help with.`)
      messageRef.current?.focus()
      return
    }
    // Email is the reply channel — always required in support mode.
    const emailValue = props.identityEmail ?? supportEmail.trim()
    if (!emailValue) {
      setError(`Your email is required so we can reply.`)
      return
    }
    const failure = await props.onSubmitSupport({
      message: trimmedMessage,
      email: emailValue,
    })
    if (failure) setError(failure)
  }

  const sideClass = props.position === `bottom-left` ? `exp-left` : `exp-right`

  if (props.phase === `success`) {
    return (
      <div
        ref={panelRef}
        className={`exp-panel ${sideClass}`}
        role="dialog"
        aria-modal="true"
        aria-label={
          props.successFlavor === `support` ? `Request sent` : `Feedback sent`
        }
      >
        <div className="exp-success">
          <div
            className="exp-success-icon"
            dangerouslySetInnerHTML={{ __html: checkIconSvg }}
          />
          <div className="exp-success-title">
            {props.successFlavor === `support`
              ? `We got your request!`
              : `Thanks for the report!`}
          </div>
          <div className="exp-success-sub">
            {props.successFlavor === `support` ? (
              `Check your email — we sent you a link to track the conversation and reply.`
            ) : props.successIdentifier ? (
              props.successUrl ? (
                <>
                  Filed as{` `}
                  <a
                    className="exp-success-link"
                    href={props.successUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {props.successIdentifier}
                  </a>
                  .
                </>
              ) : (
                `Filed as ${props.successIdentifier}.`
              )
            ) : (
              `Your feedback has been sent.`
            )}
          </div>
        </div>
        <PoweredBy />
      </div>
    )
  }

  return (
    <div
      ref={panelRef}
      className={`exp-panel ${sideClass}`}
      style={props.hidden ? { display: `none` } : undefined}
      role="dialog"
      aria-modal="true"
      aria-label={viewTitles[props.view]}
    >
      <div className="exp-header">
        <div className="exp-header-lead">
          {props.canGoBack && props.view !== `home` && (
            <button
              type="button"
              className="exp-back"
              aria-label="Back"
              onClick={props.onBack}
              dangerouslySetInnerHTML={{ __html: backIconSvg }}
            />
          )}
          <h2>{viewTitles[props.view]}</h2>
        </div>
        <button
          className="exp-close"
          aria-label="Close"
          onClick={props.onClose}
          dangerouslySetInnerHTML={{ __html: closeIconSvg }}
        />
      </div>

      {props.view === `home` && (
        <div className="exp-body">
          <div className="exp-home-sub">How can we help?</div>
          <button
            type="button"
            className="exp-mode-card"
            onClick={() => props.onPickMode(`feedback`)}
          >
            <span className="exp-mode-title">Give feedback</span>
            <span className="exp-mode-sub">
              Report a bug or share an idea — screenshot included.
            </span>
          </button>
          <button
            type="button"
            className="exp-mode-card"
            onClick={() => props.onPickMode(`support`)}
          >
            <span className="exp-mode-title">Get help</span>
            <span className="exp-mode-sub">
              Ask us anything — we'll reply by email.
            </span>
          </button>
        </div>
      )}

      {props.view === `support` && (
        <form className="exp-body" onSubmit={submitSupport}>
          <div className="exp-field">
            <label htmlFor="exp-message">How can we help?</label>
            <textarea
              id="exp-message"
              ref={messageRef}
              className="exp-textarea"
              placeholder="Describe your question or problem…"
              maxLength={10_000}
              value={message}
              onInput={(event) =>
                setMessage((event.target as HTMLTextAreaElement).value)
              }
            />
          </div>
          {!props.identityEmail && (
            <div className="exp-field">
              <label htmlFor="exp-support-email">Email</label>
              <input
                id="exp-support-email"
                className="exp-input"
                type="email"
                placeholder="you@example.com"
                maxLength={320}
                value={supportEmail}
                onInput={(event) =>
                  setSupportEmail((event.target as HTMLInputElement).value)
                }
              />
            </div>
          )}
          {error && <div className="exp-error">{error}</div>}
          <div className="exp-footer" style={{ padding: `0`, border: `none` }}>
            <button
              type="submit"
              className="exp-submit"
              disabled={props.phase === `submitting`}
            >
              {props.phase === `submitting` ? `Sending…` : `Send request`}
            </button>
          </div>
        </form>
      )}

      {props.view === `feedback` && (
        <form className="exp-body" onSubmit={submit}>
          <div className="exp-shot">
            {props.screenshot ? (
              <>
                <img
                  src={props.screenshot.objectUrl}
                  alt="Screenshot of this page"
                />
                <div className="exp-shot-actions">
                  <button
                    type="button"
                    className="exp-chip"
                    onClick={props.onAnnotate}
                  >
                    Annotate
                  </button>
                  <button
                    type="button"
                    className="exp-chip"
                    onClick={props.onRetake}
                  >
                    Retake
                  </button>
                  <button
                    type="button"
                    className="exp-chip"
                    onClick={props.onRemoveScreenshot}
                  >
                    Remove
                  </button>
                </div>
              </>
            ) : (
              <div className="exp-shot-empty">
                <span>
                  {props.captureFailed
                    ? `Screenshot couldn't be captured.`
                    : `Attach a screenshot of this page.`}
                </span>
                <button
                  type="button"
                  className="exp-chip"
                  onClick={props.onCapture}
                >
                  {props.captureFailed ? `Try again` : `Take screenshot`}
                </button>
              </div>
            )}
          </div>

          <div className="exp-field">
            <label htmlFor="exp-title">Title</label>
            <input
              id="exp-title"
              ref={titleRef}
              className="exp-input"
              placeholder="Something's broken on this page…"
              maxLength={500}
              value={title}
              onInput={(event) =>
                setTitle((event.target as HTMLInputElement).value)
              }
            />
          </div>

          <div className="exp-field">
            <label htmlFor="exp-description">Details</label>
            <textarea
              id="exp-description"
              className="exp-textarea"
              placeholder="What happened? What did you expect?"
              maxLength={10_000}
              value={description}
              onInput={(event) =>
                setDescription((event.target as HTMLTextAreaElement).value)
              }
            />
          </div>

          {!props.identityEmail && (
            <div className="exp-field">
              <label htmlFor="exp-email">
                {props.emailRequired ? `Email` : `Email (optional)`}
              </label>
              <input
                id="exp-email"
                className="exp-input"
                type="email"
                placeholder="you@example.com"
                maxLength={320}
                value={email}
                onInput={(event) =>
                  setEmail((event.target as HTMLInputElement).value)
                }
              />
            </div>
          )}

          {error && <div className="exp-error">{error}</div>}

          <div className="exp-footer" style={{ padding: `0`, border: `none` }}>
            <button
              type="submit"
              className="exp-submit"
              disabled={props.phase === `submitting` || props.flattening}
            >
              {props.phase === `submitting`
                ? `Sending…`
                : props.flattening
                  ? `Preparing screenshot…`
                  : `Send feedback`}
            </button>
          </div>
        </form>
      )}
      <PoweredBy />
    </div>
  )
}
