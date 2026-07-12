import { useEffect, useRef, useState } from "preact/hooks"
import type { Screenshot } from "./App"

const closeIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12"/></svg>`
const checkIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>`

export function Panel(props: {
  phase: `open` | `submitting` | `success`
  // Mounted-but-invisible while the annotation editor is open, so the typed
  // form fields survive.
  hidden?: boolean
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
  onRetake(): void
  onAnnotate(): void
  onRemoveScreenshot(): void
  onSubmit(form: {
    title: string
    description: string
    email: string
  }): Promise<string | null>
}) {
  const [title, setTitle] = useState(``)
  const [description, setDescription] = useState(``)
  const [email, setEmail] = useState(``)
  const [error, setError] = useState<string | null>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const titleRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    titleRef.current?.focus()
  }, [])

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
    const message = await props.onSubmit({
      title: trimmedTitle,
      description: description.trim(),
      email: email.trim(),
    })
    if (message) setError(message)
  }

  const sideClass = props.position === `bottom-left` ? `exp-left` : `exp-right`

  if (props.phase === `success`) {
    return (
      <div
        ref={panelRef}
        className={`exp-panel ${sideClass}`}
        role="dialog"
        aria-modal="true"
        aria-label="Feedback sent"
      >
        <div className="exp-success">
          <div
            className="exp-success-icon"
            dangerouslySetInnerHTML={{ __html: checkIconSvg }}
          />
          <div className="exp-success-title">Thanks for the report!</div>
          <div className="exp-success-sub">
            {props.successIdentifier ? (
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
      aria-label="Send feedback"
    >
      <div className="exp-header">
        <h2>Send feedback</h2>
        <button
          className="exp-close"
          aria-label="Close"
          onClick={props.onClose}
          dangerouslySetInnerHTML={{ __html: closeIconSvg }}
        />
      </div>

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
                <button type="button" className="exp-chip" onClick={props.onRetake}>
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
                  : `No screenshot attached.`}
              </span>
              <button type="button" className="exp-chip" onClick={props.onRetake}>
                {props.captureFailed ? `Try again` : `Capture screenshot`}
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
    </div>
  )
}
