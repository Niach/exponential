import { useEffect, useState } from "react"
import { IcCopy, IcGithub, IcServer, IcShield, IcZap } from "./icons"

export function SectionTag({ num, label }: { num: string; label: string }) {
  return (
    <div className="section-tag">
      <span className="num">{num}</span>
      <span className="line" />
      <span
        className="num"
        style={{
          textTransform: `uppercase`,
          letterSpacing: `0.12em`,
          color: `var(--accent)`,
        }}
      >
        {label}
      </span>
      <span className="line" />
    </div>
  )
}

export function HostTerminal() {
  const [step, setStep] = useState(0)
  useEffect(() => {
    const id = setInterval(() => setStep((s) => (s + 1) % 6), 1400)
    return () => clearInterval(id)
  }, [])

  return (
    <div className="terminal">
      <div className="terminal-bar">
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: `50%`,
            background: `oklch(0.7 0.2 22)`,
          }}
        />
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: `50%`,
            background: `oklch(0.78 0.16 75)`,
          }}
        />
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: `50%`,
            background: `oklch(0.72 0.15 155)`,
          }}
        />
        <span style={{ marginLeft: 8 }}>~/exponential — bash</span>
      </div>
      <div className="terminal-body">
        <span className="term-comment"># 1. clone the repo</span>
        {`\n`}
        <span className="term-prompt">$ </span>
        <span className="term-cmd">
          git clone https://github.com/Niach/exponential
        </span>
        {`\n`}
        <span className="term-prompt">$ </span>
        <span className="term-cmd">cd exponential</span>
        {`\n`}
        {`\n`}
        <span className="term-comment">
          # 2. configure your .env (DB, auth, optional OIDC)
        </span>
        {`\n`}
        <span className="term-prompt">$ </span>
        <span className="term-cmd">cp .env.example .env</span>
        {`\n`}
        {`\n`}
        <span className="term-comment"># 3. bring up the stack</span>
        {`\n`}
        <span className="term-prompt">$ </span>
        <span className="term-cmd">docker compose up -d</span>
        {`\n`}
        <span className="term-out">[+] postgres   </span>
        <span className="term-ok">healthy</span>
        {`\n`}
        <span className="term-out">[+] electric   </span>
        <span className="term-ok">healthy</span>
        {`\n`}
        <span className="term-out">[+] minio      </span>
        <span className="term-ok">healthy</span>
        {`\n`}
        <span className="term-out">[+] caddy      </span>
        <span className="term-ok">healthy</span>
        {`\n`}
        {`\n`}
        <span className="term-comment">
          # 4. apply migrations and you're up
        </span>
        {`\n`}
        <span className="term-prompt">$ </span>
        <span className="term-cmd">bun migrate</span>
        {`\n`}
        <span className="term-out">  Listening on </span>
        <span className="term-ok">https://localhost:3000</span>
        {`\n`}
        <span className="term-prompt">$ </span>
        {step % 2 === 0 && <span className="cursor-blink" />}
      </div>
    </div>
  )
}

export function FeatureGrid() {
  return (
    <div className="features">
      <div className="feature">
        <span className="feature-icon">
          <IcShield size={20} />
        </span>
        <h3>OIDC out of the box</h3>
        <p>
          Better Auth handles sessions and email/password. Plug in any OIDC
          provider — Authentik, Keycloak, Google — by setting four environment
          variables.
        </p>
      </div>

      <div className="feature">
        <span className="feature-icon">
          <IcZap size={20} />
        </span>
        <h3>Real-time, optimistic</h3>
        <p>
          Electric streams Postgres changes to every connected client.
          Mutations apply locally and reconcile through the database — no
          spinners, no stale lists.
        </p>
      </div>

      <div className="feature">
        <span className="feature-icon">
          <IcServer size={20} />
        </span>
        <h3>Your data, your servers</h3>
        <p>
          One docker-compose file: Postgres, Electric, MinIO, Caddy. No SaaS
          dependencies, no telemetry, no vendor lock-in.
        </p>
      </div>
    </div>
  )
}

export function RepoCard() {
  return (
    <div className="repo-card">
      <div className="repo-head">
        <IcGithub size={18} />
        <span className="repo-owner">Niach</span>
        <span style={{ color: `var(--fg-dim)` }}>/</span>
        <span className="repo-name">exponential</span>
      </div>

      <div className="repo-meta">
        <span>
          <IcShield size={12} /> MIT license
        </span>
        <span>v0.5.1</span>
      </div>

      <p
        style={{
          margin: 0,
          fontSize: 13.5,
          color: `var(--fg-muted)`,
          lineHeight: 1.6,
        }}
      >
        A real-time issue tracker built with TanStack Start, Electric SQL,
        Drizzle, and Better Auth. Read the source, fork it, run it on your own
        infrastructure.
      </p>
    </div>
  )
}

export function OssCopy() {
  return (
    <div
      style={{
        display: `flex`,
        flexDirection: `column`,
        justifyContent: `center`,
        gap: 18,
      }}
    >
      <div className="section-eyebrow" style={{ marginBottom: 0 }}>
        Open source
      </div>
      <h2 className="section-title" style={{ marginBottom: 0 }}>
        Read every line. Run every line.
      </h2>
      <p className="section-sub" style={{ marginBottom: 0 }}>
        The full source lives in one repo under MIT. Audit it, fork it, run it
        untouched on your own metal.
      </p>
      <div style={{ display: `flex`, gap: 10, flexWrap: `wrap` }}>
        <a
          className="btn btn-primary"
          href="https://github.com/Niach/exponential"
        >
          <IcGithub size={14} /> View on GitHub
        </a>
      </div>
    </div>
  )
}

export function CopyBlock() {
  const [copied, setCopied] = useState(false)
  const cmd =
    `git clone https://github.com/Niach/exponential && cd exponential && docker compose up -d`
  const onCopy = () => {
    navigator.clipboard?.writeText(cmd)
    setCopied(true)
    setTimeout(() => setCopied(false), 1400)
  }
  return (
    <div className="copy-cmd">
      <code>{cmd}</code>
      <button className="copy-btn" onClick={onCopy}>
        <IcCopy size={12} /> {copied ? `Copied` : `Copy`}
      </button>
    </div>
  )
}
