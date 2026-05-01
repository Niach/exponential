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
            Exponential · self-hosted issue tracker · last updated 2026-04-27
          </p>

          <p style={prose}>
            These terms govern your use of the Exponential application
            (“Exponential”, “the app”). Exponential is published by Daniel
            Strähhuber as a personal, open-source project. There is no hosted
            commercial service — every deployment is operated by the person or
            organization who installed it.
          </p>

          <h2 style={h2Style}>License</h2>
          <p style={prose}>
            Exponential is released under the MIT License. The full license
            text and source are at{` `}
            <a
              href="https://github.com/Niach/exponential"
              style={linkStyle}
            >
              github.com/Niach/exponential
            </a>
            . You are free to use, modify, and redistribute the software
            subject to the terms of that license.
          </p>

          <h2 style={h2Style}>No warranty</h2>
          <p style={prose}>
            The software is provided “as is”, without warranty of any kind,
            express or implied, including but not limited to the warranties of
            merchantability, fitness for a particular purpose, and
            non-infringement. In no event shall the author be liable for any
            claim, damages, or other liability, whether in an action of
            contract, tort, or otherwise, arising from, out of, or in
            connection with the software or its use.
          </p>

          <h2 style={h2Style}>Self-hosted operation</h2>
          <p style={prose}>
            When you install Exponential on your own infrastructure, you are
            the operator of that instance. You are responsible for its
            availability, security, backups, lawful use, and for any data
            stored within it. The author of Exponential has no access to your
            data and provides no support obligations.
          </p>

          <h2 style={h2Style}>Third-party integrations</h2>
          <p style={prose}>
            Exponential can optionally connect to third-party services such as
            Google Calendar via OAuth, at the user’s explicit choice. Use of
            those services is also governed by the third party’s own terms and
            policies. See the{` `}
            <a href="/privacy/" style={linkStyle}>
              Privacy Policy
            </a>{` `}
            for how Exponential handles data from connected accounts.
          </p>

          <h2 style={h2Style}>Acceptable use</h2>
          <p style={prose}>
            Do not use Exponential to violate any law, infringe on the rights
            of others, distribute malware, or attempt to compromise any system
            you do not own or are not authorized to access.
          </p>

          <h2 style={h2Style}>Changes</h2>
          <p style={prose}>
            These terms may be updated to reflect changes to the software or
            its integrations. Material changes will be reflected by updating
            the “last updated” date at the top of this page.
          </p>

          <h2 style={h2Style}>Contact</h2>
          <p style={prose}>
            Questions about these terms:{` `}
            <a href="mailto:danny@straehhuber.com" style={linkStyle}>
              danny@straehhuber.com
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
  color: `var(--accent)`,
  textDecoration: `underline`,
  textUnderlineOffset: 3,
} as const
