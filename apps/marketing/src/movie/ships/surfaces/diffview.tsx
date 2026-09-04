// surfaces/diffview.tsx — PrDiffPane: the PR-diff CENTER SCREEN the Reviews
// rows open in today's desktop (EXP-181/EXP-388 — the per-issue Changes tab
// and its "Open terminal in worktree" button are gone).
//
// EXP-706 reshaped it (pixel truth: shots/review-diff/desktop.webp +
// crates/ui/src/pr_diff.rs): a review DETAIL bar over a 768px centred column —
// line 1 the PR glyph, the identifier and the mono branch with the merge
// CLUSTER on the right (close · the two-stage Merge · open-on-GitHub ·
// undock), line 2 the PR state, the file count and the `+`/`−` totals. The
// `#N` sub is gone (PR numbers are leaving the surfaces). Below it the diff
// renders per-file COLLAPSED CARDS instead of one endless flat list; the open
// card holds the side-by-side diff with paint-in rows, hot-flash add/del tints
// and a tiny TS syntax tinter.
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

// EXP-706 header cluster glyphs (lucide, stroke 2).
const GitMergeIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <circle cx="18" cy="18" r="3" />
    <circle cx="6" cy="6" r="3" />
    <path d="M6 21V9a9 9 0 0 0 9 9" />
  </svg>
)

const XIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M18 6 6 18" />
    <path d="m6 6 12 12" />
  </svg>
)

const ExternalLinkIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M15 3h6v6" />
    <path d="M10 14 21 3" />
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
  </svg>
)

const UndockIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M7 7h10v10" />
    <path d="M7 17 17 7" />
  </svg>
)

const ChevronIcon: React.FC<{ size?: number; up?: boolean }> = ({ size = 12, up }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d={up ? "m18 15-6-6-6 6" : "m6 9 6 6 6-6"} />
  </svg>
)

const Spinner: React.FC<{ frame: number; size?: number }> = ({ frame, size = 11 }) => (
  <span style={{ display: "flex", rotate: `${(frame * 24) % 360}deg`, flex: "none" }}>
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round">
      <path d="M21 12a9 9 0 1 1-6.2-8.56" />
    </svg>
  </span>
)

// ── The header's merge cluster (pr_merge.rs drives the SAME two-click
// machinery the Reviews list does, so the metrics match ReviewsTool's) ───────
export type PrMergeState = `rest` | `confirm` | `merging` | `gone`

const MERGE_H = 28
const MERGE_W: Record<Exclude<PrMergeState, `gone`>, number> = {
  rest: 74, // ⑂ + "Merge"
  confirm: 134, // "Confirm merge" (danger, no glyph)
  merging: 106, // spinner + "Merging…"
}
const MERGE_PREV: Record<Exclude<PrMergeState, `gone`>, Exclude<PrMergeState, `gone`>> = {
  rest: `rest`,
  confirm: `rest`,
  merging: `confirm`,
}
const MERGE_LABEL: Record<Exclude<PrMergeState, `gone`>, string> = {
  rest: `Merge`,
  confirm: `Confirm merge`,
  merging: `Merging…`,
}

// Pane geometry — the 768px column cap (pr_diff.rs DIFF_COLUMN_W) and the
// header rows above the file cards. Exported so the segments' cursor keys can
// target the Merge pill as it morphs.
const COLUMN_W = 768
const HEAD_PAD_TOP = 12
const ICON_BTN = 26
const CLUSTER_GAP = 8

export const prDiffMergeCenter = (
  paneX: number,
  paneY: number,
  paneW: number,
  state: Exclude<PrMergeState, `gone`> = `rest`
): { x: number; y: number } => {
  const columnRight = paneX + (paneW - Math.min(COLUMN_W, paneW)) / 2 + Math.min(COLUMN_W, paneW)
  const mergeRight = columnRight - 2 * (ICON_BTN + CLUSTER_GAP)
  return {
    x: mergeRight - MERGE_W[state] / 2,
    y: paneY + HEAD_PAD_TOP + MERGE_H / 2,
  }
}

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

export type DiffFileSpec = {
  status: string
  path: string
  selected?: boolean
  /** Collapsed-card `+N −N` (the open card rolls `fileStats` instead). */
  add?: number
  del?: number
}
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
  /** The header's two-stage Merge pill (EXP-706). `gone` = merged, no pill. */
  mergeState?: PrMergeState
  /** Global frame the CURRENT mergeState began — drives the 6f width morph. */
  mergeMorphAt?: number
  /** Cursor is over the Merge pill. */
  mergeHover?: boolean
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
  mergeState = `rest`,
  mergeMorphAt,
  mergeHover,
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

  // The header's Merge pill morphs its width between the three labels.
  const mergeMorphT =
    mergeMorphAt === undefined
      ? 1
      : interpolate(frame, [mergeMorphAt, mergeMorphAt + 6], [0, 1], {
          ...CLAMP,
          easing: EASE,
        })
  const mergeShown = mergeState !== `gone`
  const mergeKey = (mergeState === `gone` ? `rest` : mergeState) as Exclude<
    PrMergeState,
    `gone`
  >
  const mergeW = interpolate(
    mergeMorphT,
    [0, 1],
    [MERGE_W[MERGE_PREV[mergeKey]], MERGE_W[mergeKey]],
    CLAMP
  )
  const danger = mergeState === `confirm`
  const mergeFg = danger
    ? C.destructive
    : mergeState === `merging`
      ? C.muted
      : C.text

  // Line 2 carries the PR's totals across every file; each card carries its
  // own (`pr_diff.rs` sums `issues.prFiles`).
  const otherFiles = (files as readonly DiffFileSpec[]).filter(
    (f: DiffFileSpec, i: number) =>
      !(f.selected === true || (i === 0 && !files.some((x: DiffFileSpec) => x.selected === true)))
  )
  const totalAdd =
    add + otherFiles.reduce((sum: number, f: DiffFileSpec) => sum + (f.add ?? 0), 0)
  const totalDel =
    del + otherFiles.reduce((sum: number, f: DiffFileSpec) => sum + (f.del ?? 0), 0)

  const iconButton = (node: React.ReactNode, key: string) => (
    <span
      key={key}
      style={{
        width: ICON_BTN,
        height: ICON_BTN,
        flex: "none",
        borderRadius: 999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: C.muted,
      }}
    >
      {node}
    </span>
  )

  // One collapsed file card; `children` (the diff) makes it the open one.
  const fileCard = (
    file: DiffFileSpec,
    stats: { add: number; del: number },
    children?: React.ReactNode
  ) => (
    <div
      key={file.path}
      style={{
        boxSizing: "border-box",
        borderRadius: 10,
        border: `1px solid ${C.strokeCard}`,
        backgroundColor: C.fillCard,
        overflow: "hidden",
      }}
    >
      <div
        style={{
          height: 40,
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "0 12px",
        }}
      >
        <span
          style={{
            fontFamily: MONO_FONT,
            fontSize: 11,
            fontWeight: 700,
            color: file.status === "A" ? C.diffAdd : C.statusInProgress,
          }}
        >
          {file.status}
        </span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontFamily: MONO_FONT,
            fontSize: 12,
            fontWeight: 600,
            color: C.text,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {file.path}
        </span>
        <span style={{ fontFamily: MONO_FONT, fontSize: 11.5, color: C.diffAdd }}>
          {`+${stats.add}`}
        </span>
        <span style={{ fontFamily: MONO_FONT, fontSize: 11.5, color: C.diffDel }}>
          {`−${stats.del}`}
        </span>
        <span style={{ color: C.dim, display: "flex" }}>
          <ChevronIcon up={children !== undefined} />
        </span>
      </div>
      {children === undefined ? null : (
        <div style={{ borderTop: `1px solid ${C.strokeRow}` }}>{children}</div>
      )}
    </div>
  )

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        fontFamily: UI_FONT,
        overflow: "hidden",
      }}
    >
      <div
        style={{
          width: COLUMN_W,
          maxWidth: "100%",
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* ── The review DETAIL bar (EXP-706): identifier + branch, the merge
               cluster right, then the PR state / file count / totals ── */}
        <div style={{ flexShrink: 0, paddingTop: HEAD_PAD_TOP }}>
          <div
            style={{
              height: MERGE_H,
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            <span style={{ color: C.diffAdd, display: "flex", alignItems: "center" }}>
              <GitPullRequestIcon />
            </span>
            <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>
              {head.identifier}
            </span>
            <span style={{ fontFamily: MONO_FONT, fontSize: 11.5, color: C.muted }}>
              {head.branch}
            </span>
            <div style={{ flex: 1 }} />
            {iconButton(<XIcon />, "close")}
            {mergeShown ? (
              <span
                style={{
                  width: mergeW,
                  height: MERGE_H,
                  flex: "none",
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  gap: 6,
                  borderRadius: 999,
                  border: `1px solid ${
                    danger
                      ? `rgba(255,100,103,${0.35 + 0.35 * mergeMorphT})`
                      : C.strokeStrong
                  }`,
                  backgroundColor:
                    mergeHover && mergeState === `rest`
                      ? C.fillActive
                      : "transparent",
                  color: mergeFg,
                  fontSize: 12,
                  fontWeight: 500,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                }}
              >
                {mergeState === `merging` ? <Spinner frame={frame} /> : null}
                {mergeState === `rest` ? <GitMergeIcon size={13} /> : null}
                {MERGE_LABEL[mergeKey]}
              </span>
            ) : null}
            {iconButton(<ExternalLinkIcon />, "github")}
            {iconButton(<UndockIcon />, "undock")}
          </div>
          <div
            style={{
              marginTop: 3,
              display: "flex",
              alignItems: "center",
              gap: 10,
              fontSize: 12,
              color: C.muted,
            }}
          >
            <span style={{ color: C.text }}>
              {mergeState === `gone` ? "Merged" : "Open"}
            </span>
            <span>
              {files.length === 1 ? "1 file" : `${files.length} files`}
            </span>
            <span style={{ fontFamily: MONO_FONT, color: C.diffAdd }}>{`+${totalAdd}`}</span>
            <span style={{ fontFamily: MONO_FONT, color: C.diffDel }}>{`−${totalDel}`}</span>
          </div>
        </div>

        {/* ── The per-file cards; the selected one is open ── */}
        <div
          style={{
            flex: 1,
            minHeight: 0,
            paddingTop: 12,
            overflow: "hidden",
          }}
        >
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              translate: `0px ${-scrollY}px`,
            }}
          >
            {files.map((file: DiffFileSpec, fi: number) => {
              const open =
                file.selected === true ||
                (fi === 0 &&
                  !files.some((f: DiffFileSpec) => f.selected === true))
              if (!open)
                return fileCard(file, {
                  add: file.add ?? 0,
                  del: file.del ?? 0,
                })
              return fileCard(
                file,
                { add, del },
                <div style={{ opacity: revealO(paintAt) }}>
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
              )
            })}
          </div>
        </div>
      </div>
    </div>
  )
}
