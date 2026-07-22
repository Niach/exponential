import { SiteShell } from "./components/SiteShell"

export function TermsPage() {
  return (
    <SiteShell>
      <section style={{ padding: `64px 0 96px` }}>
        <div className="shell" style={{ maxWidth: 760 }}>
          <h1
            style={{
              fontSize: 40,
              fontWeight: 600,
              letterSpacing: `-0.03em`,
              margin: `0 0 8px`,
            }}
          >
            Terms of Service
          </h1>
          <p
            style={{
              fontFamily: `var(--font-mono)`,
              fontSize: 12,
              color: `var(--fg-dim)`,
              margin: `0 0 40px`,
            }}
          >
            Exponential · issue tracker · last updated 2026-07-21
          </p>

          <p style={prose}>
            These terms govern your use of the Exponential application
            (“Exponential”, “the app”), whether accessed via the hosted cloud
            service at app.exponential.at or self-hosted on your own
            infrastructure. Exponential is published by Dennis Strähhuber.
          </p>

          <h2 style={h2Style}>License</h2>
          <p style={prose}>
            Exponential is source-available under the Exponential Small Team
            License 1.0 (ESTL-1.0). The full license text and source are at{` `}
            <a href="https://github.com/Niach/exponential" style={linkStyle}>
              github.com/Niach/exponential
            </a>
            . You may use, modify, and redistribute the software subject to the
            terms of that license. Use in production is free while your company
            and its affiliates have fewer than 10 total individuals working as
            employees and independent contractors; at 10 or more you need a
            commercial license. Evaluation, development, and testing are free at
            any size. Nobody may offer the software to third parties as a hosted
            or managed service.
          </p>

          <h2 style={h2Style}>No warranty</h2>
          <p style={prose}>
            The software is provided “as is”, without warranty of any kind,
            express or implied, including but not limited to the warranties of
            merchantability, fitness for a particular purpose, and
            non-infringement. In no event shall the author be liable for any
            claim, damages, or other liability, whether in an action of
            contract, tort, or otherwise, arising from, out of, or in connection
            with the software or its use.
          </p>

          <h2 style={h2Style}>Cloud service</h2>
          <p style={prose}>
            The hosted cloud instance at app.exponential.at is offered on a free
            tier and paid subscription plans (see{` `}
            <a href="/pricing/" style={linkStyle}>
              Pricing
            </a>
            ). Paid subscriptions are billed per seat through Creem, our
            merchant of record. Availability, pricing, and feature scope may
            change. Data stored on the cloud instance is processed on
            infrastructure operated by the author; see the{` `}
            <a href="/privacy/" style={linkStyle}>
              Privacy Policy
            </a>
            {` `}
            for details.
          </p>

          <h2 style={h2Style}>Self-hosted operation</h2>
          <p style={prose}>
            You may self-host Exponential on your own infrastructure, subject to
            the license above. Running it for your own company is free while
            that company and its affiliates have fewer than 10 total individuals
            working as employees and independent contractors; at 10 or more you
            need a commercial license (
            <a href="mailto:support@exponential.at" style={linkStyle}>
              support@exponential.at
            </a>
            ). You may not offer a self-hosted instance to third parties as a
            hosted or managed service. As operator of a self-hosted instance,
            you are responsible for its availability, security, backups, lawful
            use, and for any data stored within it. The author has no access to
            self-hosted data and provides no support obligations.
          </p>

          <h2 style={h2Style}>Third-party integrations</h2>
          <p style={prose}>
            Exponential can optionally connect to third-party services such as
            GitHub, via the Exponential GitHub App, at the user’s explicit
            choice. Use of those services is also governed by the third party’s
            own terms and policies. See the{` `}
            <a href="/privacy/" style={linkStyle}>
              Privacy Policy
            </a>
            {` `}
            for how Exponential handles data from connected accounts.
          </p>

          <h2 style={h2Style}>Acceptable use</h2>
          <p style={prose}>
            Do not use Exponential to violate any law, infringe on the rights of
            others, distribute malware, or attempt to compromise any system you
            do not own or are not authorized to access.
          </p>

          <h2 style={h2Style}>Changes</h2>
          <p style={prose}>
            These terms may be updated to reflect changes to the software or its
            integrations. Material changes will be reflected by updating the
            “last updated” date at the top of this page.
          </p>

          <h2 style={h2Style}>Contact</h2>
          <p style={prose}>
            Questions about these terms:{` `}
            <a href="mailto:support@exponential.at" style={linkStyle}>
              support@exponential.at
            </a>
            .
          </p>
        </div>
      </section>
    </SiteShell>
  )
}

const prose = {
  fontSize: 15,
  lineHeight: 1.7,
  color: `var(--fg-muted)`,
  margin: `0 0 18px`,
} as const

const h2Style = {
  fontSize: 20,
  fontWeight: 500,
  letterSpacing: `-0.02em`,
  color: `var(--fg)`,
  margin: `40px 0 12px`,
} as const

const linkStyle = {
  color: `var(--fg)`,
  textDecoration: `underline`,
  textUnderlineOffset: 3,
} as const
