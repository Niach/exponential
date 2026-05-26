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

function BotIcon({ size = 12 }: { size?: number }) {
  return (
    <MiniIcon
      size={size}
      d="M12 8V4H8m8 4v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-4V8h12zM2 14h2M20 14h2M15 11v2M9 11v2"
      color="var(--accent)"
    />
  )
}

function SparklesIcon({ size = 12 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="var(--accent)"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
      <path d="M20 3v4M22 5h-4" />
    </svg>
  )
}

function CopyIcon({ size = 11 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect x={9} y={9} width={13} height={13} rx={2} />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
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
   DocsMockupAgentSettings
   ═══════════════════════════════════════════ */
export function DocsMockupAgentSettings() {
  return (
    <div className="docs-mockup">
      <div className="dm-agent-settings">
        {/* Header */}
        <div className="dm-agent-header">
          <BotIcon size={14} />
          <span style={{ fontWeight: 600, fontSize: 13 }}>Agent Members</span>
        </div>

        {/* Install command */}
        <div className="dm-agent-cmd">
          <code>
            npx exponential-agent --url https://app.exponenti
          </code>
          <span className="dm-agent-cmd-fade" />
          <button className="dm-agent-cmd-copy" type="button">
            <CopyIcon size={10} />
          </button>
        </div>

        {/* Create form */}
        <div className="dm-agent-create">
          <div className="dm-agent-input">Agent name</div>
          <button className="dm-agent-add-btn" type="button">
            Add agent member
          </button>
        </div>

        {/* Existing agent */}
        <div className="dm-agent-row">
          <span className="dm-agent-avatar">
            <SparklesIcon size={11} />
          </span>
          <span className="dm-agent-info">
            <span className="dm-agent-name">
              Claude
              <span className="dm-agent-badge">agent</span>
            </span>
            <span className="dm-agent-email">claude@agents</span>
          </span>
          <span className="dm-agent-meta">Last seen: just now</span>
          <span className="dm-agent-actions">
            <button className="dm-agent-action" type="button">
              Regenerate
            </button>
            <button className="dm-agent-action dm-danger" type="button">
              Revoke
            </button>
          </span>
        </div>
      </div>
    </div>
  )
}

/* ═══════════════════════════════════════════
   DocsMockupPlanComment
   ═══════════════════════════════════════════ */
export function DocsMockupPlanComment() {
  return (
    <div className="docs-mockup">
      <div className="dm-plan-comment">
        {/* Header */}
        <div className="dm-plan-head">
          <span className="dm-plan-avatar">
            <SparklesIcon size={11} />
          </span>
          <strong>Claude</strong>
          <span className="dm-plan-tag">Plan &middot; rev 1</span>
          <span className="dm-plan-time">just now</span>
        </div>

        {/* Body */}
        <div className="dm-plan-body">
          <p>
            1. Add webhook dispatch table{`\n`}
            2. Wire up issue mutation events{`\n`}
            3. Sign payloads with HMAC-SHA256
          </p>
        </div>

        {/* Actions */}
        <div className="dm-plan-actions">
          <button className="dm-plan-btn is-primary" type="button">
            Approve
          </button>
          <button className="dm-plan-btn" type="button">
            Request changes
          </button>
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
