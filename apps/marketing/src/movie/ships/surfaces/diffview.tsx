// surfaces/diffview.tsx — PrDiffPane: the PR-diff CENTER SCREEN the Reviews
// rows open in today's desktop (EXP-181/EXP-388 — the per-issue Changes tab
// and its "Open terminal in worktree" button are gone). Thin header row
// (PR icon · identifier · title · "#N · branch", no buttons), then per-file
// header bands over a side-by-side diff with paint-in rows, hot-flash
// add/del tints and a tiny TS syntax tinter.
// The component fills its parent (position:absolute inset 0) — the assembler
// places it over the center pane (window-local x 304–1568, below the tab strip).

import React from "react"
import { interpolate } from "remotion"
import { C, EASE, MONO_FONT, UI_FONT } from "../theme"
import { DIFF_FILES, DIFF_ROWS, HERO, type DiffRow } from "../fixtures"
import { rollNum } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// Base code color matched to the ref diff (context lines are slightly dimmer
// than C.text; the contract uses the same value for terminal prose).
const CODE_FG = "#d4d4d4"

// ── Tiny inline icons (lucide-style, stroke 1.6, currentColor) ───────────────
const GitPullRequestIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <circle cx="6" cy="6" r="3" />
    <circle cx="18" cy="18" r="3" />
    <path d="M13 6h3a2 2 0 0 1 2 2v7" />
    <path d="M6 9v12" />
  </svg>
)

// ── Tiny TS syntax tinter (keywords / strings / numbers / comments) ──────────
const TS_RE =
  /(\/\/[^\n]*)|(`(?:[^`\\]|\\.)*`|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|\b(const|let|var|function|return|new|typeof|import|export|from|if|else|for|while|null|undefined|true|false|async|await)\b|\b(\d[\d_]*(?:\.\d+)?)\b/g

const tintTs = (text: string): React.ReactNode[] => {
  const out: React.ReactNode[] = []
  let last = 0
  let key = 0
  TS_RE.lastIndex = 0
  for (let m = TS_RE.exec(text); m; m = TS_RE.exec(text)) {
    if (m.index > last) out.push(text.slice(last, m.index))
    const color = m[1] ? C.synComment : m[2] ? C.synString : m[3] ? C.synKeyword : C.synNumber
    out.push(
      <span key={key} style={{ color }}>
        {m[0]}
      </span>,
    )
    key += 1
    last = m.index + m[0].length
    if (m[0].length === 0) TS_RE.lastIndex += 1
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

// ── Pair the unified fixture rows into side-by-side display rows ─────────────
type Side = { n?: number; text: string; kind: "ctx" | "add" | "del" }
type Painted = { t: "hunk"; text: string } | { t: "pair"; l: Side | null; r: Side | null }

const buildPairs = (rows: readonly DiffRow[]): Painted[] => {
  const out: Painted[] = []
  let dels: DiffRow[] = []
  let adds: DiffRow[] = []
  const flush = () => {
    const n = Math.max(dels.length, adds.length)
    for (let i = 0; i < n; i++) {
      const d = dels[i]
      const a = adds[i]
      out.push({
        t: "pair",
        l: d ? { n: d.old, text: d.text, kind: "del" } : null,
        r: a ? { n: a.new, text: a.text, kind: "add" } : null,
      })
    }
    dels = []
    adds = []
  }
  for (const r of rows) {
    if (r.t === "hunk") {
      flush()
      out.push({ t: "hunk", text: r.text })
    } else if (r.t === "del") {
      dels.push(r)
    } else if (r.t === "add") {
      adds.push(r)
    } else {
      flush()
      out.push({
        t: "pair",
        l: { n: r.old, text: r.text, kind: "ctx" },
        r: { n: r.new, text: r.text, kind: "ctx" },
      })
    }
  }
  flush()
  return out
}

const PAIRS = buildPairs(DIFF_ROWS)

// Selected file's own header stats — consistent with the HERO_SESSION Update
// result ("Added 29 lines, removed 11 lines") and the hunk span (-48,11 +48,29).
const FILE_STATS = { add: 29, del: 11 } as const

const ROW_H = 18
const GUTTER_W = 34

// One diff cell (gutter + code). `null` cell = filler blank on the unpaired side.
const DiffCell: React.FC<{ side: Side | null; bgAlpha: number }> = ({ side, bgAlpha }) => {
  const bg =
    side === null
      ? "rgba(255,255,255,0.03)" // filler — faint glass wash
      : side.kind === "add"
        ? `rgba(34,197,94,${bgAlpha})`
        : side.kind === "del"
          ? `rgba(239,68,68,${bgAlpha})`
          : "transparent"
  const gutterColor =
    side === null
      ? "transparent"
      : side.kind === "add"
        ? "rgba(34,197,94,0.8)"
        : side.kind === "del"
          ? "rgba(239,68,68,0.8)"
          : C.dim
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", height: ROW_H, backgroundColor: bg }}>
      <span
        style={{
          width: GUTTER_W,
          flexShrink: 0,
          textAlign: "right",
          paddingRight: 8,
          fontFamily: MONO_FONT,
          fontSize: 11,
          color: gutterColor,
        }}
      >
        {side?.n ?? ""}
      </span>
      <span
        style={{
          fontFamily: MONO_FONT,
          fontSize: 11,
          lineHeight: `${ROW_H}px`,
          whiteSpace: "pre",
          overflow: "hidden",
          color: CODE_FG,
        }}
      >
        {side ? tintTs(side.text) : ""}
      </span>
    </div>
  )
}

export type DiffFileSpec = { status: string; path: string; selected?: boolean }
export type PrDiffHead = {
  identifier: string
  title: string
  pr: number
  branch: string
}

export type PrDiffPaneProps = {
  frame: number
  /** Global frame the diff area starts painting in (file header, then 1 row/frame). Undefined = resting (all painted). */
  paintAt?: number
  /** Global frame the file band's +N −N stats start digit-rolling from 0 (12f). Undefined = resting values. */
  statsRollAt?: number
  /** Extra vertical scroll of the diff content in px (assembler-driven). */
  scrollY?: number
  /** Header content (identifier · title · "#N · branch"). Default: the ships HERO. */
  head?: PrDiffHead
  /** Changed files (band paths). Default: the ships DIFF_FILES fixture. */
  files?: readonly DiffFileSpec[]
  /** Unified diff rows (paired side-by-side here). Default: the ships DIFF_ROWS fixture. */
  rows?: readonly DiffRow[]
  /** Shown file's +N −N header-band stats. Default: the ships FILE_STATS. */
  fileStats?: { add: number; del: number }
}

const HERO_HEAD: PrDiffHead = {
  identifier: HERO.id,
  title: HERO.title,
  pr: HERO.pr,
  branch: HERO.branch,
}

export const PrDiffPane: React.FC<PrDiffPaneProps> = ({
  frame,
  paintAt,
  statsRollAt,
  scrollY = 0,
  head = HERO_HEAD,
  files = DIFF_FILES,
  rows,
  fileStats = FILE_STATS,
}) => {
  const pairs = rows === undefined ? PAIRS : buildPairs(rows)
  const add = statsRollAt === undefined ? fileStats.add : rollNum(frame, statsRollAt, statsRollAt + 12, 0, fileStats.add)
  const del = statsRollAt === undefined ? fileStats.del : rollNum(frame, statsRollAt, statsRollAt + 12, 0, fileStats.del)

  // Paint-in: file header reveals at paintAt, display row i at paintAt + 1 + i.
  const revealO = (at: number | undefined) =>
    at === undefined ? 1 : interpolate(frame, [at, at + 3], [0, 1], { ...CLAMP, easing: EASE })
  // Hot flash → settle: add/del row bg alpha 0.20 → 0.10 over 8f after its reveal.
  const tintAlpha = (at: number | undefined) =>
    at === undefined ? 0.1 : interpolate(frame, [at, at + 8], [0.2, 0.1], CLAMP)

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        flexDirection: "column",
        fontFamily: UI_FONT,
        overflow: "hidden",
      }}
    >
      {/* ── Header row (PrDiff, EXP-181): PR icon · identifier · title ·
             "#N · branch" — no buttons ── */}
      <div
        style={{
          height: 34,
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 12px",
          borderBottom: `1px solid ${C.strokeRow}`,
        }}
      >
        <span style={{ color: C.diffAdd, display: "flex", alignItems: "center" }}>
          <GitPullRequestIcon />
        </span>
        <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.muted }}>{head.identifier}</span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12.5,
            fontWeight: 500,
            color: C.text,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {head.title}
        </span>
        <span style={{ fontSize: 12, color: C.muted, flexShrink: 0 }}>
          {`#${head.pr} · `}
          <span style={{ fontFamily: MONO_FONT, fontSize: 11.5 }}>{head.branch}</span>
        </span>
      </div>

      {/* ── Body: side-by-side diff under a per-file header band (no file
             list in the real PrDiff screen) ── */}
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        {/* diff scroll area */}
        <div style={{ flex: 1, minWidth: 0, overflow: "hidden" }}>
          <div style={{ translate: `0px ${-scrollY}px` }}>
            {/* file header band (26px, muted@30 bar, status badge, bold mono
                path, rolling +N −N right) */}
            <div
              style={{
                height: 26,
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "0 12px",
                backgroundColor: "rgba(255,255,255,0.08)",
                borderBottom: `1px solid ${C.strokeRow}`,
                opacity: revealO(paintAt),
              }}
            >
              <span
                style={{
                  fontFamily: MONO_FONT,
                  fontSize: 11,
                  fontWeight: 700,
                  color: files[0]?.status === "A" ? C.diffAdd : C.statusInProgress,
                }}
              >
                {files[0]?.status ?? "M"}
              </span>
              <span style={{ fontFamily: MONO_FONT, fontSize: 12, fontWeight: 700, color: C.text }}>
                {files[0]?.path ?? ""}
              </span>
              <div style={{ flex: 1 }} />
              <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.diffAdd }}>{`+${add}`}</span>
              <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.diffDel }}>{`−${del}`}</span>
            </div>

            {pairs.map((row, i) => {
              const at = paintAt === undefined ? undefined : paintAt + 1 + i
              if (row.t === "hunk") {
                return (
                  <div
                    key={i}
                    style={{
                      height: ROW_H,
                      display: "flex",
                      alignItems: "center",
                      paddingLeft: 8,
                      backgroundColor: C.hunkBg,
                      opacity: revealO(at),
                    }}
                  >
                    <span
                      style={{
                        fontFamily: MONO_FONT,
                        fontSize: 11,
                        color: C.hunkFg,
                        whiteSpace: "pre",
                        overflow: "hidden",
                      }}
                    >
                      {row.text}
                    </span>
                  </div>
                )
              }
              const alpha = tintAlpha(at)
              return (
                <div key={i} style={{ display: "flex", opacity: revealO(at) }}>
                  <DiffCell side={row.l} bgAlpha={alpha} />
                  <div style={{ width: 1, flexShrink: 0, backgroundColor: C.strokeRow }} />
                  <DiffCell side={row.r} bgAlpha={alpha} />
                </div>
              )
            })}
          </div>
        </div>
      </div>
    </div>
  )
}
