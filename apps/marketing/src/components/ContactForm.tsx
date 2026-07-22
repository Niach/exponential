import { useEffect, useState } from "react"
import type { FormEvent } from "react"
import { LINKS } from "../lib/links"

/* Enterprise contact form (pricing page, #enterprise-contact). Posts JSON to
   the cloud /api/contact endpoint (201 {ok:true} on success, 400 validation,
   429 rate limited, 503 mail transport down). SSR-deterministic: prerendered
   by scripts/prerender.tsx and hydrated, so the initial markup must be
   identical on server and client — no window/document/Date outside handlers. */

type SubmitState = `idle` | `submitting` | `success` | `rateLimited` | `error`

const SALES_MAILTO = `mailto:support@exponential.at?subject=Exponential%20Enterprise`

function MailtoFallback() {
  return (
    <a className="contact-mailto" href={SALES_MAILTO}>
      support@exponential.at
    </a>
  )
}

export function ContactForm() {
  const [name, setName] = useState(``)
  const [email, setEmail] = useState(``)
  const [company, setCompany] = useState(``)
  const [message, setMessage] = useState(``)
  /* Honeypot — real visitors never see or fill it; bots that do get
     silently accepted-and-dropped server-side (same trick as the widget).
     The field name is deliberately non-address-like so browser autofill
     (which populates off-screen inputs and largely ignores autoComplete="off"
     for profile fields like URL/website) can't fill it and lose a real lead. */
  const [contactNonce, setContactNonce] = useState(``)
  const [state, setState] = useState<SubmitState>(`idle`)
  /* Pre-hydration guard — until React attaches handleSubmit, a native submit
     would GET-navigate and leak the field values into the URL. Both server
     and client first-render disabled (deterministic), then the mount effect
     enables the button once the JSON submit path is live. A disabled default
     button also blocks Enter-key implicit submission per the HTML spec. */
  const [hydrated, setHydrated] = useState(false)
  useEffect(() => {
    setHydrated(true)
  }, [])

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault()
    if (state === `submitting`) return
    setState(`submitting`)
    try {
      const res = await fetch(LINKS.app.contact, {
        method: `POST`,
        headers: { "content-type": `application/json` },
        credentials: `omit`,
        body: JSON.stringify({
          name,
          email,
          company,
          message,
          contactNonce,
          source: `pricing-enterprise`,
        }),
      })
      if (res.ok) {
        setState(`success`)
      } else if (res.status === 429) {
        setState(`rateLimited`)
      } else {
        setState(`error`)
      }
    } catch {
      setState(`error`)
    }
  }

  if (state === `success`) {
    return (
      <div className="contact-form contact-success" role="status">
        <p className="contact-success-title">Thanks — message sent.</p>
        <p className="contact-success-sub">
          We&apos;ll get back to you at the email you provided, usually within
          one business day.
        </p>
      </div>
    )
  }

  /* method/action are a pre-hydration backstop: if a submit somehow fires
     natively anyway, POST keeps the field values out of the URL/history. */
  return (
    <form
      className="contact-form"
      method="post"
      action="/contact/"
      onSubmit={handleSubmit}
    >
      <div className="contact-grid">
        <label className="contact-field">
          <span className="contact-label">Name</span>
          <input
            className="contact-input"
            type="text"
            name="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            autoComplete="name"
          />
        </label>
        <label className="contact-field">
          <span className="contact-label">Work email</span>
          <input
            className="contact-input"
            type="email"
            name="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            autoComplete="email"
          />
        </label>
        <label className="contact-field contact-field-wide">
          <span className="contact-label">Company</span>
          <input
            className="contact-input"
            type="text"
            name="company"
            value={company}
            onChange={(e) => setCompany(e.target.value)}
            autoComplete="organization"
          />
        </label>
      </div>
      <label className="contact-field">
        <span className="contact-label">Message</span>
        <textarea
          className="contact-input contact-textarea"
          name="message"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          required
          rows={5}
          placeholder="Team size, deployment plans, anything else we should know."
        />
      </label>
      {/* Honeypot — off-screen, out of the tab order, hidden from AT. */}
      <input
        className="contact-hp"
        type="text"
        name="contact_nonce"
        value={contactNonce}
        onChange={(e) => setContactNonce(e.target.value)}
        tabIndex={-1}
        autoComplete="one-time-code"
        aria-hidden="true"
      />
      {state === `rateLimited` && (
        <p className="contact-error" role="alert">
          Too many requests right now — please try again in a bit, or email us
          directly at <MailtoFallback />.
        </p>
      )}
      {state === `error` && (
        <p className="contact-error" role="alert">
          Something went wrong sending your message. Please try again, or email
          us directly at <MailtoFallback />.
        </p>
      )}
      <div className="contact-actions">
        <button
          className="btn btn-primary"
          type="submit"
          disabled={!hydrated || state === `submitting`}
        >
          {state === `submitting` ? `Sending…` : `Send message`}
        </button>
      </div>
    </form>
  )
}
