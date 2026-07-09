import { SiteShell } from "./components/SiteShell"

export function ImprintPage() {
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
            Imprint
          </h1>
          <p
            style={{
              fontFamily: `var(--font-mono)`,
              fontSize: 12,
              color: `var(--fg-dim)`,
              margin: `0 0 40px`,
            }}
          >
            Exponential · exponential.at · Impressum
          </p>

          <h2 style={h2Style}>Angaben gemäß § 5 DDG</h2>
          <p style={prose}>
            Dennis Strähhuber
            <br />
            Nocksteinweg 12
            <br />
            83416 Saaldorf-Surheim
            <br />
            Germany
          </p>

          <h2 style={h2Style}>Contact</h2>
          <p style={prose}>
            Email:{` `}
            <a href="mailto:dennis@straehhuber.com" style={linkStyle}>
              dennis@straehhuber.com
            </a>
          </p>

          <h2 style={h2Style}>
            Verantwortlich für den Inhalt gemäß § 18 Abs. 2 MStV
          </h2>
          <p style={prose}>
            Dennis Strähhuber
            <br />
            Nocksteinweg 12
            <br />
            83416 Saaldorf-Surheim
            <br />
            Germany
          </p>

          <h2 style={h2Style}>
            Verbraucherstreitbeilegung / Universalschlichtungsstelle
          </h2>
          <p style={prose}>
            We are not willing or obliged to participate in dispute resolution
            proceedings before a consumer arbitration board (§ 36 VSBG).
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
