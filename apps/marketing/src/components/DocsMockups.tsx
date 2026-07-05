import type { CSSProperties } from "react"

/* ── Tiny inline SVG helpers (avoid importing heavy icon lib) ── */

function MiniIcon({ d, size = 12, color }: { d: string; size?: number; color?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color ?? `currentColor`}
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={d} />
    </svg>
  )
}

function StatusCircle({ color, filled }: { color: string; filled?: boolean }) {
  return (
    <svg width={12} height={12} viewBox="0 0 24 24" fill="none">
      <circle
        cx={12}
        cy={12}
        r={9}
        stroke={color}
        strokeWidth={2}
        fill={filled ? color : `none`}
      />
      {filled && (
        <polyline
          points="8 12 11 15 16 9"
          stroke="var(--bg-card)"
          strokeWidth={2.2}
          fill="none"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      )}
    </svg>
  )
}

function HalfCircle({ color }: { color: string }) {
  return (
    <svg width={12} height={12} viewBox="0 0 24 24" fill="none">
      <circle cx={12} cy={12} r={9} stroke={color} strokeWidth={2} />
      <path d="M12 3 a9 9 0 0 1 0 18 z" fill={color} />
    </svg>
  )
}

function PriorityBars({
  level,
  color,
}: {
  level: 1 | 2 | 3 | 4
  color: string
}) {
  const bars = 4
  return (
    <svg width={12} height={12} viewBox="0 0 16 16">
      {Array.from({ length: bars }).map((_, i) => (
        <rect
          key={i}
          x={1 + i * 4}
          y={12 - (i + 1) * 3}
          width={3}
          height={(i + 1) * 3}
          rx={0.5}
          fill={i < level ? color : `oklch(0.3 0 0)`}
        />
      ))}
    </svg>
  )
}

const s = {
  dim: { color: `var(--fg-dim)` } as CSSProperties,
}

/* ═══════════════════════════════════════════
   DocsMockupSidebar
   ═══════════════════════════════════════════ */
export function DocsMockupSidebar() {
  const projects = [
    { name: `Exponential`, color: `oklch(0.62 0.18 280)`, active: true },
    { name: `Marketing`, color: `oklch(0.68 0.17 155)`, active: false },
    { name: `Mobile`, color: `oklch(0.72 0.16 60)`, active: false },
  ]

  return (
    <div className="docs-mockup" style={{ display: `inline-block` }}>
      <div className="docs-mockup-sidebar">
        {/* Workspace pill */}
        <div className="dm-ws-pill">
          <span className="dm-ws-avatar">A</span>
          <span className="dm-ws-name">Acme</span>
          <span style={{ ...s.dim, fontSize: 11, marginLeft: `auto` }}>
            <MiniIcon d="M6 9l6 6 6-6" size={10} color="var(--fg-dim)" />
          </span>
        </div>

        {/* Projects */}
        <div className="dm-section">
          <div className="dm-section-label">
            <span>Projects</span>
            <span className="dm-add-btn">+</span>
          </div>
          {projects.map((p) => (
            <div
              key={p.name}
              className={`dm-item${p.active ? ` is-active` : ``}`}
            >
              <span
                className="dm-dot"
                style={{ background: p.color }}
              />
              <span>{p.name}</span>
            </div>
          ))}
        </div>

        {/* Spacer */}
        <div style={{ flex: 1 }} />

        {/* User */}
        <div className="dm-user">
          <span className="dm-user-avatar">D</span>
          <span className="dm-user-email">danny@acme.io</span>
        </div>
      </div>
    </div>
  )
}

/* ═══════════════════════════════════════════
   DocsMockupIssueList
   ═══════════════════════════════════════════ */
export function DocsMockupIssueList() {
  const groups = [
    {
      label: `In Progress`,
      count: 2,
      icon: <HalfCircle color="var(--status-progress)" />,
      rows: [
        {
          pri: { level: 3 as const, color: `var(--priority-high)` },
          id: `EXP-14`,
          title: `Add webhook dispatch table`,
          label: { name: `backend`, color: `oklch(0.62 0.18 280)` },
          status: <HalfCircle color="var(--status-progress)" />,
        },
        {
          pri: { level: 2 as const, color: `var(--priority-medium)` },
          id: `EXP-12`,
          title: `Mobile push notification deep links`,
          label: { name: `mobile`, color: `oklch(0.72 0.16 60)` },
          status: <HalfCircle color="var(--status-progress)" />,
        },
      ],
    },
    {
      label: `Todo`,
      count: 3,
      icon: <StatusCircle color="var(--status-todo)" />,
      rows: [
        {
          pri: { level: 4 as const, color: `var(--priority-urgent)` },
          id: `EXP-18`,
          title: `Fix duplicate issue numbers on concurrent creates`,
          label: { name: `bug`, color: `oklch(0.65 0.22 22)` },
          status: <StatusCircle color="var(--status-todo)" />,
        },
        {
          pri: { level: 2 as const, color: `var(--priority-medium)` },
          id: `EXP-16`,
          title: `Google Calendar two-way sync`,
          label: { name: `integration`, color: `oklch(0.68 0.17 155)` },
          status: <StatusCircle color="var(--status-todo)" />,
        },
        {
          pri: { level: 1 as const, color: `var(--priority-low)` },
          id: `EXP-15`,
          title: `Export issues to CSV`,
          label: null,
          status: <StatusCircle color="var(--status-todo)" />,
        },
      ],
    },
  ]

  return (
    <div className="docs-mockup">
      <div className="dm-issue-list">
        {groups.map((g) => (
          <div key={g.label}>
            <div className="dm-group-header">
              {g.icon}
              <span className="dm-group-title">{g.label}</span>
              <span className="dm-group-count">{g.count}</span>
            </div>
            {g.rows.map((r) => (
              <div key={r.id} className="dm-row">
                <span className="dm-row-pri">
                  <PriorityBars level={r.pri.level} color={r.pri.color} />
                </span>
                <span className="dm-row-ident">{r.id}</span>
                <span className="dm-row-status">{r.status}</span>
                <span className="dm-row-title">{r.title}</span>
                <span className="dm-row-trail">
                  {r.label && (
                    <span className="dm-label">
                      <span
                        className="dm-label-dot"
                        style={{ background: r.label.color }}
                      />
                      {r.label.name}
                    </span>
                  )}
                </span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}

/* ═══════════════════════════════════════════
   DocsMockupRepositories — workspace repo registry
   ═══════════════════════════════════════════ */
function GitBranchIcon({ size = 12 }: { size?: number }) {
  return (
    <MiniIcon
      size={size}
      d="M6 3v12M18 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 9a9 9 0 0 0 9 9"
      color="var(--accent)"
    />
  )
}

export function DocsMockupRepositories() {
  const repos = [
    {
      project: `Exponential`,
      repo: `niach/exponential`,
      meta: `main · cloned`,
    },
    {
      project: `Marketing`,
      repo: `niach/exponential-site`,
      meta: `main · cloned`,
    },
  ]
  return (
    <div className="docs-mockup">
      <div className="dm-agent-settings">
        {/* Header */}
        <div className="dm-agent-header">
          <GitBranchIcon size={14} />
          <span style={{ fontWeight: 600, fontSize: 13 }}>Repositories</span>
        </div>

        <p
          style={{
            margin: 0,
            fontSize: 11.5,
            color: `var(--fg-dim)`,
            lineHeight: 1.5,
          }}
        >
          Every project is backed by one GitHub repository. The desktop app
          clones it automatically the first time you open the project.
        </p>

        {/* Connected repos */}
        {repos.map((r) => (
          <div className="dm-agent-row" key={r.repo}>
            <span className="dm-agent-avatar">
              <GitBranchIcon size={11} />
            </span>
            <span className="dm-agent-info">
              <span className="dm-agent-name">
                {r.project}
                <span className="dm-agent-badge">repo</span>
              </span>
              <span className="dm-agent-email">{r.repo}</span>
            </span>
            <span className="dm-agent-meta">{r.meta}</span>
            <span className="dm-agent-actions">
              <button className="dm-agent-action" type="button">
                Configure
              </button>
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

/* ═══════════════════════════════════════════
   DocsMockupDesktopCoding — mini desktop IDE window:
   issue list beside the embedded coding terminal
   ═══════════════════════════════════════════ */
export function DocsMockupDesktopCoding() {
  const lines = [
    { cls: `tl-cmd`, text: `claude` },
    { cls: `tl-dim`, text: `worktree exp/EXP-181 · branch created` },
    { cls: `tl-plan`, text: `Plan: add webhooks table + dispatchWebhook()` },
    { cls: `tl-tool`, text: `▸ Edit src/lib/webhooks.ts` },
    { cls: `tl-tool`, text: `▸ Bash bun test` },
    { cls: `tl-ok`, text: `  ✓ 24 tests passed` },
    { cls: `tl-ok`, text: `✓ Pushed exp/EXP-181 · opened PR #214` },
  ]
  return (
    <div className="docs-mockup">
      <div className="dm-desktop">
        <div className="dm-desktop-bar">
          <span className="dm-desktop-dots">
            <span />
            <span />
            <span />
          </span>
          <span className="dm-desktop-title">Exponential — Acme</span>
        </div>
        <div className="dm-desktop-body">
          <div className="dm-desktop-side">
            <div className="dm-desktop-side-label">Issues</div>
            <div className="dm-desktop-issue is-active">
              <span className="dm-row-ident">EXP-181</span>
              <span>Add webhook events</span>
            </div>
            <div className="dm-desktop-issue">
              <span className="dm-row-ident">EXP-186</span>
              <span>Fix inbox badge count</span>
            </div>
            <div className="dm-desktop-issue">
              <span className="dm-row-ident">EXP-190</span>
              <span>Rate-limit invites</span>
            </div>
          </div>
          <div className="dm-desktop-term">
            {lines.map((l) => (
              <div key={l.text} className={`term-line ${l.cls}`}>
                {l.text}
              </div>
            ))}
            <div className="term-line">
              <span className="caret" aria-hidden />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

/* ═══════════════════════════════════════════
   DocsMockupSteerPhone — a phone streaming the
   live desktop coding terminal, with a steer input
   ═══════════════════════════════════════════ */
export function DocsMockupSteerPhone() {
  const lines = [
    { cls: `tl-dim`, text: `worktree exp/EXP-181` },
    { cls: `tl-plan`, text: `Plan: add webhooks table` },
    { cls: `tl-tool`, text: `▸ Edit src/lib/webhooks.ts` },
    { cls: `tl-ok`, text: `  ✓ 24 tests passed` },
  ]
  return (
    <div
      className="docs-mockup"
      style={{
        display: `inline-block`,
        background: `transparent`,
        border: `none`,
      }}
    >
      <div className="dm-phone">
        <div className="dm-phone-screen">
          {/* Status bar */}
          <div className="dm-phone-status">
            <span className="dm-phone-time">9:41</span>
            <span className="dm-phone-island" />
            <span className="dm-phone-icons">
              <svg width={10} height={10} viewBox="0 0 16 16">
                <path
                  d="M1 12l3-3 3 3 3-5 3 5 2-2"
                  stroke="white"
                  strokeWidth={1.5}
                  fill="none"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              <span className="dm-phone-battery">
                <span className="dm-phone-battery-fill" />
              </span>
            </span>
          </div>

          {/* Header */}
          <div className="dm-phone-header">EXP-181 · Live session</div>

          {/* Streamed terminal + steer input */}
          <div className="dm-phone-body" style={{ gap: 10 }}>
            <div
              style={{
                fontFamily: `var(--font-mono)`,
                fontSize: 9.5,
                lineHeight: 1.5,
                background: `rgba(0,0,0,0.35)`,
                border: `0.5px solid rgba(255,255,255,0.08)`,
                borderRadius: 10,
                padding: `10px 12px`,
              }}
            >
              {lines.map((l) => (
                <div
                  key={l.text}
                  className={`term-line ${l.cls}`}
                  style={{ minHeight: `1.5em` }}
                >
                  {l.text}
                </div>
              ))}
              <div className="term-line" style={{ minHeight: `1.5em` }}>
                <span className="caret" aria-hidden />
              </div>
            </div>
            <div className="dm-phone-input" style={{ fontSize: 12 }}>
              <span style={{ color: `var(--accent)`, marginRight: 6 }}>›</span>
              <span style={{ color: `rgba(255,255,255,0.4)` }}>
                Type to steer…
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

/* ═══════════════════════════════════════════
   DocsMockupPhoneConnect
   ═══════════════════════════════════════════ */
export function DocsMockupPhoneConnect() {
  return (
    <div
      className="docs-mockup"
      style={{
        display: `inline-block`,
        background: `transparent`,
        border: `none`,
      }}
    >
      <div className="dm-phone">
        <div className="dm-phone-screen">
          {/* Status bar */}
          <div className="dm-phone-status">
            <span className="dm-phone-time">9:41</span>
            <span className="dm-phone-island" />
            <span className="dm-phone-icons">
              <svg width={10} height={10} viewBox="0 0 16 16">
                <path
                  d="M1 12l3-3 3 3 3-5 3 5 2-2"
                  stroke="white"
                  strokeWidth={1.5}
                  fill="none"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              <span className="dm-phone-battery">
                <span className="dm-phone-battery-fill" />
              </span>
            </span>
          </div>

          {/* Header */}
          <div className="dm-phone-header">Add server</div>

          {/* URL input */}
          <div className="dm-phone-body">
            <label className="dm-phone-label">Server URL</label>
            <div className="dm-phone-input">
              <span style={{ color: `rgba(255,255,255,0.4)` }}>https://</span>
              app.exponential.at
            </div>
            <button className="dm-phone-connect" type="button">
              Connect
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
