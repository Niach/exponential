// surfaces/releases.tsx — ReleasesTool (sidebar list) + ReleaseDetailTool
// (drill-in detail with progress bar, Shipped pill, PR chip, grouped member
// issues + the S11 cascade-into-Done). Pixel truth:
// ref/desktop-release-detail-dialog.png (left pane — real release detail).
// Both components fill their parent (position:absolute inset 0) — the
// assembler places them over the 260px tool sidebar (window-local x 44–304,
// below the 38px top bar). All coordinates below are pane-local px.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, UI_FONT } from "../theme"
import { BOARD, RELEASE, type IssueStatus, type Priority } from "../fixtures"
import { rollNum } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const E = { ...CLAMP, easing: EASE } as const

// ── Tiny inline icons (lucide-style, stroke 1.6–2, currentColor) ──────────────
const RocketIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
    <path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91.09z" />
    <path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z" />
    <path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0" />
    <path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5" />
  </svg>
)

const PlusIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
    <path d="M12 5v14" />
    <path d="M5 12h14" />
  </svg>
)

const PlayIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <polygon points="6 3 20 12 6 21 6 3" />
  </svg>
)

const EllipsisIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" stroke="none">
    <circle cx="5" cy="12" r="1.6" />
    <circle cx="12" cy="12" r="1.6" />
    <circle cx="19" cy="12" r="1.6" />
  </svg>
)

const ChevronLeftIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="m15 18-6-6 6-6" />
  </svg>
)

const ChevronDownIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="m6 9 6 6 6-6" />
  </svg>
)

const CalendarIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="4" width="18" height="18" rx="2" />
    <path d="M16 2v4" />
    <path d="M8 2v4" />
    <path d="M3 10h18" />
  </svg>
)

const GitPrIcon: React.FC<{ size?: number }> = ({ size = 11 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <circle cx="6" cy="6" r="3" />
    <circle cx="18" cy="18" r="3" />
    <path d="M13 6h3a2 2 0 0 1 2 2v7" />
    <path d="M6 9v12" />
  </svg>
)

const GitMergeIcon: React.FC<{ size?: number }> = ({ size = 11 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <circle cx="18" cy="18" r="3" />
    <circle cx="6" cy="6" r="3" />
    <path d="M6 21V9a9 9 0 0 0 9 9" />
  </svg>
)

// ── Status / priority glyphs (match the real board rows in the ref shots) ─────
const StatusGlyph: React.FC<{ status: IssueStatus; size?: number }> = ({ status, size = 14 }) => {
  if (status === "done") {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={C.statusDone} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="9.5" />
        <path d="m8.5 12.5 2.4 2.4 4.8-5.2" />
      </svg>
    )
  }
  if (status === "in_progress") {
    // Yellow stopwatch/timer — matches the real In Progress glyph.
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={C.statusInProgress} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
        <line x1="10" y1="2" x2="14" y2="2" />
        <line x1="12" y1="14" x2="15" y2="11" />
        <circle cx="12" cy="14" r="8" />
      </svg>
    )
  }
  if (status === "backlog") {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={C.statusBacklog} strokeWidth={2} strokeLinecap="round">
        <circle cx="12" cy="12" r="9.5" strokeDasharray="3.5 3.5" />
      </svg>
    )
  }
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={C.statusTodo} strokeWidth={2}>
      <circle cx="12" cy="12" r="9.5" />
    </svg>
  )
}

const PRIO_BAR_OFF = "rgba(255,255,255,0.16)"

const PrioGlyph: React.FC<{ p: Priority; size?: number }> = ({ p, size = 14 }) => {
  if (p === "none") {
    return (
      <svg width={size} height={size} viewBox="0 0 16 16">
        <rect x="3" y="7.25" width="10" height="1.5" rx="0.75" fill={C.dim} />
      </svg>
    )
  }
  if (p === "urgent") {
    return (
      <svg width={size} height={size} viewBox="0 0 16 16">
        <rect x="1.5" y="1.5" width="13" height="13" rx="3" fill={C.prioUrgent} />
        <rect x="7.25" y="4" width="1.5" height="5" rx="0.75" fill="#fff" />
        <circle cx="8" cy="11.25" r="1" fill="#fff" />
      </svg>
    )
  }
  const filled = p === "low" ? 1 : p === "medium" ? 2 : 3
  const color = p === "low" ? C.prioLow : p === "medium" ? C.prioMedium : C.prioHigh
  return (
    <svg width={size} height={size} viewBox="0 0 16 16">
      <rect x="2" y="9" width="3" height="5" rx="0.75" fill={filled >= 1 ? color : PRIO_BAR_OFF} />
      <rect x="6.5" y="6" width="3" height="8" rx="0.75" fill={filled >= 2 ? color : PRIO_BAR_OFF} />
      <rect x="11" y="3" width="3" height="11" rx="0.75" fill={filled >= 3 ? color : PRIO_BAR_OFF} />
    </svg>
  )
}

// DS avatar — violet like the real app's deterministic member color (ref rows).
const MiniAvatar: React.FC<{ size?: number }> = ({ size = 16 }) => (
  <div
    style={{
      width: size,
      height: size,
      borderRadius: "50%",
      backgroundColor: "rgba(168,85,247,0.30)",
      border: "1px solid rgba(168,85,247,0.40)",
      color: "#e9d5ff",
      fontSize: 7.5,
      fontWeight: 600,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      flexShrink: 0,
    }}
  >
    DS
  </div>
)

// ── Fixture lookups (BOARD is base truth; EXP-141/145 exist only in RELEASE) ──
const rowTitle = (id: string): string => {
  for (const r of BOARD) if (r.id === id) return r.title
  for (const d of RELEASE.dialogIssues) if (d.id === id) return d.title
  return id
}
const rowPrio = (id: string): Priority => {
  for (const r of BOARD) if (r.id === id) return r.priority
  return "none"
}
const rowHasAvatar = (id: string): boolean => {
  for (const r of BOARD) if (r.id === id) return r.assignee !== undefined
  return false
}

// ── ReleasesTool — sidebar list (S9 part 1) ───────────────────────────────────
export type ReleasesToolProps = {
  frame: number
  /** Hover tint on the v0.12 row: fades in at `at`, back out at `out`. */
  hover?: { at: number; out?: number }
  /** Optional drill-out: slides the list left 24px + fades (pairs with ReleaseDetailTool.drillAt). */
  exitAt?: number
  /** Done count in the sub-line (default 3 — pre-ship fixture). */
  doneCount?: number
}

export const ReleasesTool: React.FC<ReleasesToolProps> = ({ frame, hover, exitAt, doneCount = RELEASE.doneAtS9 }) => {
  const hoverO =
    hover === undefined
      ? 0
      : interpolate(frame, [hover.at, hover.at + 4], [0, 1], CLAMP) *
        (hover.out === undefined ? 1 : interpolate(frame, [hover.out, hover.out + 4], [1, 0], CLAMP))
  const exitO = exitAt === undefined ? 1 : interpolate(frame, [exitAt, exitAt + 12], [1, 0], E)
  const exitX = exitAt === undefined ? 0 : interpolate(frame, [exitAt, exitAt + 12], [0, -24], E)
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        overflow: "hidden",
        fontFamily: UI_FONT,
        opacity: exitO,
        translate: `${exitX}px 0px`,
      }}
    >
      {/* Header: 🚀 Releases · + (h34) */}
      <div
        style={{
          height: 34,
          display: "flex",
          alignItems: "center",
          gap: 7,
          padding: "0 10px",
          borderBottom: `1px solid ${C.borderSoft}`,
        }}
      >
        <span style={{ color: C.muted, display: "flex" }}>
          <RocketIcon size={13} />
        </span>
        <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>Releases</span>
        <div style={{ flex: 1 }} />
        <span style={{ color: C.muted, display: "flex", padding: 4 }}>
          <PlusIcon size={13} />
        </span>
      </div>
      {/* v0.12 row (two lines, radius 6) — pane-local rect x6..254 y38..85 */}
      <div style={{ padding: "4px 6px 0" }}>
        <div
          style={{
            borderRadius: 6,
            padding: "7px 8px",
            backgroundColor: `rgba(38,38,38,${0.85 * hoverO})`,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ color: C.muted, display: "flex" }}>
              <RocketIcon size={15} />
            </span>
            <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>{RELEASE.name}</span>
          </div>
          <div style={{ marginTop: 3, paddingLeft: 23, fontSize: 11, color: C.muted }}>
            {`Target ${RELEASE.target} · ${doneCount} of ${RELEASE.total} done`}
          </div>
        </div>
      </div>
    </div>
  )
}

// ── ReleaseDetailTool — drill-in detail (S9–S11) ──────────────────────────────
export type ReleaseProgressStep = {
  at: number // global frame the fill segment starts
  from: number // done count before
  to: number // done count after
  dur?: number // fill duration (default 8f; storyboard uses 6f ticks, 12f final)
}

export type ReleaseDetailToolProps = {
  frame: number
  /** Slide-in from +24px + fade over 12f. Hidden before this frame. */
  drillAt?: number
  /** Progress bar fill steps (sorted by `at`). Empty array = resting at 3 of 8. */
  progress: ReleaseProgressStep[]
  /** Green "Shipped" pill POP next to the name; rocket tints green; meta flips to "Shipped Jul 12". */
  shippedAt?: number
  /** S11: remaining rows cascade into the green Done group (3f staggers). */
  cascadeDoneAt?: number
  /** Resting done count before any progress step (default fixtures.RELEASE.doneAtS9). */
  baseDone?: number
  /** Flip a row's status glyph to green done at a global frame (pre-cascade merges). */
  statusFlipAt?: Record<string, number>
  /** PR chip next to the meta date. Default when `shippedAt` set: merged chip at shippedAt+10. */
  prChip?: { at: number; mergedAt?: number }
  /** Hover tint behind the ▷ Start coding action (cursor approach). */
  hoverStartCoding?: { at: number; out?: number }
}

// Layout constants (pane-local px)
const HEAD_H = 34
const ACT_H = 30
const SUMMARY_TOP = HEAD_H + ACT_H // 64
const LIST_TOP = SUMMARY_TOP + 64 // summary block: pt10 + meta20 + mt10 + bar12 + pb12
const ENTRY_H = 28

type GroupKey = "in_progress" | "todo" | "done"
const GROUPS: { key: GroupKey; label: string; tint: string; ids: readonly string[] }[] = [
  { key: "in_progress", label: "In Progress", tint: C.tintInProgress, ids: ["EXP-139"] },
  { key: "todo", label: "Todo", tint: C.tintTodo, ids: ["EXP-141", "EXP-143", "EXP-144", "EXP-145"] },
  { key: "done", label: "Done", tint: C.tintDone, ids: ["EXP-138", "EXP-140", "EXP-142"] },
]
const MOVERS: readonly string[] = ["EXP-139", "EXP-141", "EXP-143", "EXP-144", "EXP-145"]

// Pre-cascade y (grouped) and post-cascade y (everything in Done) per entry key.
const PRE_Y = new Map<string, number>()
{
  let y = 0
  for (const g of GROUPS) {
    PRE_Y.set(`h:${g.key}`, y)
    y += ENTRY_H
    for (const id of g.ids) {
      PRE_Y.set(`r:${id}`, y)
      y += ENTRY_H
    }
  }
}
const POST_Y = new Map<string, number>()
{
  let y = 0
  POST_Y.set("h:done", y)
  y += ENTRY_H
  for (const id of ["EXP-138", "EXP-140", "EXP-142", ...MOVERS]) {
    POST_Y.set(`r:${id}`, y)
    y += ENTRY_H
  }
}

const progressValue = (frame: number, steps: readonly ReleaseProgressStep[], base: number): number => {
  let v = base
  for (const s of steps) {
    if (frame < s.at) continue
    v = interpolate(frame, [s.at, s.at + (s.dur ?? 8)], [s.from, s.to], E)
  }
  return v
}

export const ReleaseDetailTool: React.FC<ReleaseDetailToolProps> = ({
  frame,
  drillAt,
  progress,
  shippedAt,
  cascadeDoneAt,
  baseDone = RELEASE.doneAtS9,
  statusFlipAt,
  prChip,
  hoverStartCoding,
}) => {
  if (drillAt !== undefined && frame < drillAt) return null
  const drillO = drillAt === undefined ? 1 : interpolate(frame, [drillAt, drillAt + 12], [0, 1], E)
  const drillX = drillAt === undefined ? 0 : interpolate(frame, [drillAt, drillAt + 12], [24, 0], E)

  // Progress
  const done = progressValue(frame, progress, baseDone)
  const fillPct = Math.min(100, Math.max(0, (done / RELEASE.total) * 100))
  const doneLabel = `${Math.round(done)} of ${RELEASE.total} done`

  // Shipped state (the two meta date texts fade sequentially, not on top of each other)
  const shipT = shippedAt === undefined ? 0 : interpolate(frame, [shippedAt, shippedAt + 8], [0, 1], CLAMP)
  const pillScale =
    shippedAt === undefined || frame < shippedAt ? 0 : spring({ frame: frame - shippedAt, fps: 30, config: POP })

  // PR chip (defaults to a merged chip riding the ship beat)
  const chip = prChip ?? (shippedAt === undefined ? undefined : { at: shippedAt + 10, mergedAt: shippedAt + 10 })
  const chipScale = chip === undefined || frame < chip.at ? 0 : spring({ frame: frame - chip.at, fps: 30, config: POP })
  const chipMerged = chip !== undefined && chip.mergedAt !== undefined && frame >= chip.mergedAt

  // Start-coding hover
  const scHoverO =
    hoverStartCoding === undefined
      ? 0
      : interpolate(frame, [hoverStartCoding.at, hoverStartCoding.at + 4], [0, 1], CLAMP) *
        (hoverStartCoding.out === undefined
          ? 1
          : interpolate(frame, [hoverStartCoding.out, hoverStartCoding.out + 4], [1, 0], CLAMP))

  // Cascade helpers
  const moverStart = (id: string): number | undefined => {
    if (cascadeDoneAt === undefined) return undefined
    const i = MOVERS.indexOf(id)
    return i >= 0 ? cascadeDoneAt + i * 3 : cascadeDoneAt
  }
  const entryY = (key: string, start: number | undefined, dur: number): number => {
    const a = PRE_Y.get(key) ?? 0
    const b = POST_Y.get(key)
    if (start === undefined || b === undefined) return a
    return interpolate(frame, [start, start + dur], [a, b], E)
  }
  const flipFrame = (id: string): number | undefined => {
    const explicit = statusFlipAt === undefined ? undefined : statusFlipAt[id]
    const mv = MOVERS.indexOf(id)
    const casc = cascadeDoneAt !== undefined && mv >= 0 ? cascadeDoneAt + mv * 3 + 4 : undefined
    if (explicit !== undefined && casc !== undefined) return Math.min(explicit, casc)
    return explicit ?? casc
  }
  const doneCount =
    cascadeDoneAt === undefined ? GROUPS[2].ids.length : rollNum(frame, cascadeDoneAt, cascadeDoneAt + 26, GROUPS[2].ids.length, RELEASE.total)

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        overflow: "hidden",
        fontFamily: UI_FONT,
        backgroundColor: C.bg,
        opacity: drillO,
        translate: `${drillX}px 0px`,
      }}
    >
      {/* Header: ‹ 🚀 v0.12 [Shipped] (h34, hairline below — ref header row) */}
      <div
        style={{
          height: HEAD_H,
          display: "flex",
          alignItems: "center",
          gap: 7,
          padding: "0 10px",
          borderBottom: `1px solid ${C.borderSoft}`,
        }}
      >
        <span style={{ color: C.muted, display: "flex", marginLeft: -2 }}>
          <ChevronLeftIcon size={15} />
        </span>
        <span style={{ display: "grid", placeItems: "center", width: 14, height: 14 }}>
          <span style={{ gridArea: "1 / 1", color: C.muted, display: "flex", opacity: 1 - shipT }}>
            <RocketIcon size={14} />
          </span>
          <span style={{ gridArea: "1 / 1", color: C.green, display: "flex", opacity: shipT }}>
            <RocketIcon size={14} />
          </span>
        </span>
        <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>{RELEASE.name}</span>
        {pillScale > 0.01 ? (
          <span
            style={{
              scale: String(pillScale),
              display: "inline-flex",
              alignItems: "center",
              height: 18,
              padding: "0 8px",
              borderRadius: 999,
              border: "1px solid rgba(34,197,94,0.40)",
              backgroundColor: "rgba(34,197,94,0.10)",
              color: C.green,
              fontSize: 11,
              fontWeight: 500,
            }}
          >
            Shipped
          </span>
        ) : null}
      </div>

      {/* Action row: + Add issues · ▷ Start coding · ⋯ (ghost actions per ref header) */}
      <div
        style={{
          height: ACT_H,
          display: "flex",
          alignItems: "center",
          gap: 14,
          padding: "0 10px",
          borderBottom: `1px solid ${C.borderSoft}`,
        }}
      >
        <span style={{ display: "inline-flex", alignItems: "center", gap: 4, fontSize: 12, color: "#d4d4d4" }}>
          <span style={{ color: C.muted, display: "flex" }}>
            <PlusIcon size={12} />
          </span>
          Add issues
        </span>
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            fontSize: 12,
            color: "#d4d4d4",
            padding: "3px 6px",
            margin: "-3px -6px",
            borderRadius: 5,
            backgroundColor: `rgba(38,38,38,${0.85 * scHoverO})`,
          }}
        >
          <span style={{ color: C.green, display: "flex" }}>
            <PlayIcon size={11} />
          </span>
          Start coding
        </span>
        <div style={{ flex: 1 }} />
        <span style={{ color: C.muted, display: "flex" }}>
          <EllipsisIcon size={14} />
        </span>
      </div>

      {/* Summary block: meta row (📅 target ↔ shipped date + PR chip) + 4px progress bar */}
      <div
        style={{
          position: "absolute",
          top: SUMMARY_TOP,
          left: 0,
          right: 0,
          height: LIST_TOP - SUMMARY_TOP,
          padding: "10px 12px 12px",
          borderBottom: `1px solid ${C.borderSoft}`,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8, height: 20 }}>
          <span style={{ display: "inline-grid", alignItems: "center" }}>
            <span
              style={{
                gridArea: "1 / 1",
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                fontSize: 12,
                color: C.muted,
                opacity: 1 - shipT,
                whiteSpace: "nowrap",
              }}
            >
              <CalendarIcon size={12} />
              {`Target ${RELEASE.target}`}
            </span>
            <span
              style={{
                gridArea: "1 / 1",
                fontSize: 12,
                color: C.muted,
                opacity: shipT,
                whiteSpace: "nowrap",
              }}
            >
              Shipped Jul 12
            </span>
          </span>
          {chipScale > 0.01 ? (
            <span
              style={{
                scale: String(chipScale),
                display: "inline-flex",
                alignItems: "center",
                gap: 4,
                height: 18,
                padding: "0 7px",
                borderRadius: 999,
                border: `1px solid ${C.border}`,
                backgroundColor: "rgba(255,255,255,0.03)",
                fontSize: 11,
                color: "#d4d4d4",
                whiteSpace: "nowrap",
              }}
            >
              <span style={{ color: chipMerged ? C.muted : C.green, display: "flex" }}>
                {chipMerged ? <GitMergeIcon size={11} /> : <GitPrIcon size={11} />}
              </span>
              {`PR #${RELEASE.pr} · ${chipMerged ? "merged" : "open"}`}
            </span>
          ) : null}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 10, height: 12 }}>
          <div
            style={{
              flex: 1,
              height: 4,
              borderRadius: 2,
              backgroundColor: "rgba(161,161,161,0.20)",
              overflow: "hidden",
            }}
          >
            <div style={{ width: `${fillPct}%`, height: 4, borderRadius: 2, backgroundColor: C.green }} />
          </div>
          <span style={{ fontSize: 11, color: C.muted, whiteSpace: "nowrap" }}>{doneLabel}</span>
        </div>
      </div>

      {/* Member issues grouped by status (absolute rows — FLIP cascade into Done) */}
      <div style={{ position: "absolute", top: LIST_TOP, left: 0, right: 0, bottom: 0 }}>
        {GROUPS.map((g) => {
          const isDoneGroup = g.key === "done"
          const headerStart = cascadeDoneAt
          const headerY = isDoneGroup ? entryY(`h:${g.key}`, headerStart, 14) : PRE_Y.get(`h:${g.key}`) ?? 0
          const headerO =
            isDoneGroup || cascadeDoneAt === undefined
              ? 1
              : interpolate(frame, [cascadeDoneAt, cascadeDoneAt + 8], [1, 0], CLAMP)
          return (
            <React.Fragment key={g.key}>
              <div
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  right: 0,
                  height: ENTRY_H,
                  translate: `0px ${headerY}px`,
                  opacity: headerO,
                  display: "flex",
                  alignItems: "center",
                  gap: 7,
                  padding: "0 10px",
                  backgroundColor: C.bg,
                  backgroundImage: `linear-gradient(${g.tint}, ${g.tint})`,
                }}
              >
                <span style={{ color: C.dim, display: "flex" }}>
                  <ChevronDownIcon size={11} />
                </span>
                <StatusGlyph status={g.key} size={13} />
                <span style={{ fontSize: 12, fontWeight: 500, color: C.text }}>{g.label}</span>
                <span style={{ fontSize: 11, color: C.dim }}>{isDoneGroup ? doneCount : g.ids.length}</span>
              </div>
              {g.ids.map((id) => {
                const mover = MOVERS.indexOf(id) >= 0
                const start = mover ? moverStart(id) : cascadeDoneAt
                const y = entryY(`r:${id}`, start, mover ? 12 : 14)
                const flip = flipFrame(id)
                const doneO =
                  g.key === "done"
                    ? 1
                    : flip === undefined
                      ? 0
                      : interpolate(frame, [flip, flip + 6], [0, 1], CLAMP)
                return (
                  <div
                    key={id}
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      right: 0,
                      height: ENTRY_H,
                      translate: `0px ${y}px`,
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      padding: "0 10px",
                      borderBottom: `1px solid ${C.borderRow}`,
                      backgroundColor: C.bg,
                    }}
                  >
                    <span style={{ width: 16, display: "flex", justifyContent: "center", flexShrink: 0 }}>
                      <PrioGlyph p={rowPrio(id)} size={13} />
                    </span>
                    <span
                      style={{
                        width: 52,
                        flexShrink: 0,
                        fontFamily: MONO_FONT,
                        fontSize: 11,
                        color: C.muted,
                        whiteSpace: "nowrap",
                      }}
                    >
                      {id}
                    </span>
                    <span style={{ width: 16, height: 14, flexShrink: 0, display: "grid", placeItems: "center" }}>
                      <span style={{ gridArea: "1 / 1", display: "flex", opacity: 1 - doneO }}>
                        <StatusGlyph status={g.key} size={13} />
                      </span>
                      <span style={{ gridArea: "1 / 1", display: "flex", opacity: doneO }}>
                        <StatusGlyph status="done" size={13} />
                      </span>
                    </span>
                    <span
                      style={{
                        flex: 1,
                        minWidth: 0,
                        fontSize: 12.5,
                        color: C.text,
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {rowTitle(id)}
                    </span>
                    {rowHasAvatar(id) ? <MiniAvatar size={16} /> : null}
                  </div>
                )
              })}
            </React.Fragment>
          )
        })}
      </div>
    </div>
  )
}
