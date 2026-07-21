import { SiteShell } from "./components/SiteShell"
import { ContactForm } from "./components/ContactForm"

export function ContactPage() {
  return (
    <SiteShell>
      <section style={{ padding: `64px 0 96px` }}>
        <div className="shell" style={{ maxWidth: 640 }}>
          <h1
            style={{
              fontSize: 40,
              fontWeight: 600,
              letterSpacing: `-0.03em`,
              margin: `0 0 8px`,
            }}
          >
            Contact sales
          </h1>
          <p
            style={{
              fontFamily: `var(--font-mono)`,
              fontSize: 12,
              color: `var(--fg-dim)`,
              margin: `0 0 40px`,
            }}
          >
            Exponential · Enterprise &amp; self-hosting
          </p>

          <p
            style={{
              fontSize: 15,
              lineHeight: 1.7,
              color: `var(--fg-muted)`,
              margin: `0 0 16px`,
            }}
          >
            Tell us about your team and how you plan to run Exponential —
            we&apos;ll get back to you within a business day.
          </p>

          <p
            style={{
              fontSize: 15,
              lineHeight: 1.7,
              color: `var(--fg-muted)`,
              margin: `0 0 28px`,
            }}
          >
            Self-hosting is free while your company and its affiliates have
            fewer than 10 total individuals working as employees and
            independent contractors. At 10 or more you need a{` `}
            <strong>commercial self-host license</strong> — this form is how
            you get one. Enterprise cloud, prioritized support, and deployment
            help start here too.
          </p>

          <ContactForm />
        </div>
      </section>
    </SiteShell>
  )
}
