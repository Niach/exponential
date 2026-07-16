// surfaces/diffview.tsx — ChangesPane: the EXP-142 Changes tab (S7).
// Header row (branch · PR · rolling stats), 240px file list, side-by-side diff
// with paint-in rows, hot-flash add/del tints and a tiny TS syntax tinter.
// Pixel truth: ref/desktop-source-control-diff.png (right half).
// The component fills its parent (position:absolute inset 0) — the assembler
// places it over the center pane (window-local x 304–1568, below the tab strip).

import React from "react"
import { interpolate } from "remotion"
import { C, EASE, MONO_FONT, UI_FONT } from "../theme"
import { DIFF_FILES, DIFF_HEADER, DIFF_ROWS, type DiffRow } from "../fixtures"
import { rollNum } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// Base code color matched to the ref diff (context lines are slightly dimmer
// than C.text; the contract uses the same value for terminal prose).
const CODE_FG = "#d4d4d4"

// ── Tiny inline icons (lucide-style, stroke 1.6, currentColor) ───────────────
const GitBranchIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <line x1="6" y1="3" x2="6" y2="15" />
    <circle cx="18" cy="6" r="3" />
    <circle cx="6" cy="18" r="3" />
    <path d="M18 9a9 9 0 0 1-9 9" />
  </svg>
)

const SquareTerminalIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
    <path d="m7 11 2-2-2-2" />
    <path d="M11 13h4" />
    <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
  </svg>
)

const EllipsisIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" stroke="none">
    <circle cx="5" cy="12" r="1.6" />
    <circle cx="12" cy="12" r="1.6" />
    <circle cx="19" cy="12" r="1.6" />
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
      ? "rgba(38,38,38,0.3)" // filler — muted @ 30%
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
export type DiffHeaderSpec = { branch: string; pr: string; stats: { files: number; add: number; del: number } }

export type ChangesPaneProps = {
  frame: number
  /** Global frame the diff area starts painting in (file header, then 1 row/frame). Undefined = resting (all painted). */
  paintAt?: number
  /** Global frame the header +120 −34 stats start digit-rolling from 0 (12f). Undefined = resting values. */
  statsRollAt?: number
  /** Extra vertical scroll of the diff content in px (assembler-driven). */
  scrollY?: number
  /** Global frame the selected file-list row tint fades in (6f). Undefined = shown from the start. */
  fileSelectAt?: number
  /** Header content (branch · PR · stats). Default: the ships DIFF_HEADER fixture. */
  header?: DiffHeaderSpec
  /** File list. Default: the ships DIFF_FILES fixture. */
  files?: readonly DiffFileSpec[]
  /** Unified diff rows (paired side-by-side here). Default: the ships DIFF_ROWS fixture. */
  rows?: readonly DiffRow[]
  /** Selected file's own +N −N header-band stats. Default: the ships FILE_STATS. */
  fileStats?: { add: number; del: number }
}

export const ChangesPane: React.FC<ChangesPaneProps> = ({
  frame,
  paintAt,
  statsRollAt,
  scrollY = 0,
  fileSelectAt,
  header = DIFF_HEADER,
  files = DIFF_FILES,
  rows,
  fileStats = FILE_STATS,
}) => {
  const pairs = rows === undefined ? PAIRS : buildPairs(rows)
  const add = statsRollAt === undefined ? header.stats.add : rollNum(frame, statsRollAt, statsRollAt + 12, 0, header.stats.add)
  const del = statsRollAt === undefined ? header.stats.del : rollNum(frame, statsRollAt, statsRollAt + 12, 0, header.stats.del)

  // Paint-in: file header reveals at paintAt, display row i at paintAt + 1 + i.
  const revealO = (at: number | undefined) =>
    at === undefined ? 1 : interpolate(frame, [at, at + 3], [0, 1], { ...CLAMP, easing: EASE })
  // Hot flash → settle: add/del row bg alpha 0.20 → 0.10 over 8f after its reveal.
  const tintAlpha = (at: number | undefined) =>
    at === undefined ? 0.1 : interpolate(frame, [at, at + 8], [0.2, 0.1], CLAMP)

  const selTint =
    fileSelectAt === undefined ? 1 : interpolate(frame, [fileSelectAt, fileSelectAt + 6], [0, 1], { ...CLAMP, easing: EASE })

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        flexDirection: "column",
        backgroundColor: C.bg,
        fontFamily: UI_FONT,
        overflow: "hidden",
      }}
    >
      {/* ── Header row: ⎇ branch · PR #214 · 5 files +120 −34 · [Open terminal in worktree] [⋯] ── */}
      <div
        style={{
          height: 42,
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 16px",
          borderBottom: `1px solid ${C.border}`,
        }}
      >
        <span style={{ color: C.muted, display: "flex", alignItems: "center" }}>
          <GitBranchIcon />
        </span>
        <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.text }}>{header.branch}</span>
        <span style={{ fontSize: 12, color: C.dim }}>·</span>
        <span style={{ fontSize: 12, color: C.muted }}>{header.pr}</span>
        <span style={{ fontSize: 12, color: C.dim }}>·</span>
        <span style={{ fontSize: 12, color: C.muted }}>{`${header.stats.files} files`}</span>
        <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.diffAdd }}>{`+${add}`}</span>
        <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.diffDel }}>{`−${del}`}</span>
        <div style={{ flex: 1 }} />
        <div
          style={{
            height: 26,
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "0 8px",
            borderRadius: 6,
            color: C.muted,
            fontSize: 12,
            fontWeight: 500,
          }}
        >
          <SquareTerminalIcon />
          <span>Open terminal in worktree</span>
        </div>
        <div
          style={{
            width: 24,
            height: 24,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            borderRadius: 6,
            color: C.muted,
          }}
        >
          <EllipsisIcon />
        </div>
      </div>

      {/* ── Body: 240px file list + side-by-side diff ── */}
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        <div style={{ width: 240, flexShrink: 0, borderRight: `1px solid ${C.border}`, paddingTop: 4 }}>
          {files.map((f) => {
            const selected = "selected" in f && f.selected === true
            return (
              <div
                key={f.path}
                style={{
                  height: 24,
                  display: "flex",
                  alignItems: "center",
                  gap: 0,
                  padding: "0 10px",
                  backgroundColor: selected ? `rgba(38,38,38,${0.6 * selTint})` : "transparent",
                }}
              >
                <span
                  style={{
                    width: 16,
                    flexShrink: 0,
                    fontFamily: MONO_FONT,
                    fontSize: 11,
                    fontWeight: 700,
                    color: f.status === "A" ? C.diffAdd : C.statusInProgress,
                  }}
                >
                  {f.status}
                </span>
                <span
                  style={{
                    fontFamily: MONO_FONT,
                    fontSize: 11,
                    color: selected ? C.text : C.muted,
                    whiteSpace: "nowrap",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                  }}
                >
                  {f.path}
                </span>
              </div>
            )
          })}
        </div>

        {/* diff scroll area */}
        <div style={{ flex: 1, minWidth: 0, overflow: "hidden" }}>
          <div style={{ translate: `0px ${-scrollY}px` }}>
            {/* file header band (26px, muted@30 bar, bold mono path, +29 −11 right) */}
            <div
              style={{
                height: 26,
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "0 12px",
                backgroundColor: "rgba(38,38,38,0.5)",
                borderBottom: `1px solid ${C.borderSoft}`,
                opacity: revealO(paintAt),
              }}
            >
              <span style={{ fontFamily: MONO_FONT, fontSize: 12, fontWeight: 700, color: C.text }}>
                {files[0]?.path ?? ""}
              </span>
              <div style={{ flex: 1 }} />
              <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.diffAdd }}>{`+${fileStats.add}`}</span>
              <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.diffDel }}>{`−${fileStats.del}`}</span>
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
                  <div style={{ width: 1, flexShrink: 0, backgroundColor: C.borderSoft }} />
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
