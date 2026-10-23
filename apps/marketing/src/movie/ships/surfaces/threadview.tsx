// surfaces/threadview.tsx — a thread as it ships (EXP-1294/1316 thread
// screen, N-12 the Dock, EXP-1321 Trace, EXP-916 edited-files card,
// EXP-895 the one diff view). Pixel truth: shots/{thread,steering,trace}/
// desktop.webp and shots/{guide,review-diff}/{desktop,web}.webp; crates/ui
// thread_screen.rs (HEADER_INSET 44, CONVERSATION_W 760, title text_2xl
// semibold, the trailing Merge PR / Stop pills), dock.rs, composer.rs,
// fixtures trace.json (sm geometry) and dock.json.
// Authored in PANEL-LOCAL LOGICAL px (render inside shell.tsx CutoutPanel).

import React from "react"
import { interpolate, spring } from "remotion"
import {
  ArrowLeft,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  CircleX,
  Code,
  FileDiff,
  FileText,
  Folder,
  GitMerge,
  LoaderCircle,
  Ellipsis,
  MessageCircle,
  Monitor,
  Pencil,
  Plus,
  Search,
  Sparkles,
  Terminal,
} from "lucide-react"
import { EASE, MONO_FONT, POP, UI_FONT } from "../theme"
import type { IssueStatus } from "../fixtures"
import { G, Ico, StatusGlyph, T } from "./shell"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

const HEADER_INSET = 44
const CONVERSATION_W = 760
const WORK_W = 896
const GUTTER = 20

// ── Transcript model ──────────────────────────────────────────────────────────
export type TranscriptItem =
  | { kind: `trace`; icon: `read` | `terminal` | `edit` | `search`; verb: string; detail: string }
  | { kind: `prose`; text: string }
  | { kind: `edits`; files: { path: string; add: number; del: number; at?: number }[] }
  | { kind: `user`; text: string; byline: string }

const TRACE_ICON = { read: FileText, terminal: Terminal, edit: Pencil, search: Search } as const

// ── Header + strip ────────────────────────────────────────────────────────────
export type MergePill = `merge` | `confirm` | `merging`

export type ThreadHeaderProps = {
  frame: number
  title: string
  issue: { id: string; title: string; status: IssueStatus }
  /** live turn: the Stop pill */
  stop?: boolean
  /** the Guide's PR is open: the Merge PR pill (two-click) */
  merge?: { state: MergePill; at?: number }
  /** which face the strip selects */
  face: `conversation` | `guide`
  width: number
}

const STRIP_TOP = 67

export const ThreadHeader: React.FC<ThreadHeaderProps> = ({
  frame,
  title,
  issue,
  stop,
  merge,
  face,
  width,
}) => {
  const morph =
    merge?.at === undefined
      ? 1
      : spring({ frame: frame - merge.at, fps: 30, config: POP })
  return (
    <>
      <div
        style={{
          position: `absolute`,
          left: HEADER_INSET,
          right: HEADER_INSET,
          top: 21,
          height: 32,
          display: `flex`,
          alignItems: `center`,
          gap: 7,
        }}
      >
        <div
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: T.xl2,
            fontWeight: 600,
            whiteSpace: `nowrap`,
            overflow: `hidden`,
            textOverflow: `ellipsis`,
          }}
        >
          {title}
        </div>
        {merge ? (
          <div
            style={{
              height: 32,
              boxSizing: `border-box`,
              display: `flex`,
              alignItems: `center`,
              gap: 6,
              padding: `0 12px`,
              borderRadius: 999,
              backgroundColor: G.primary,
              border: `1px solid transparent`,
              color: G.primaryFg,
              opacity: merge.state === `merging` ? 0.6 : 1,
              fontSize: T.sm,
              fontWeight: 500,
              whiteSpace: `nowrap`,
              scale: String(0.94 + 0.06 * Math.min(1, morph)),
            }}
          >
            <span style={{ display: `flex`, color: merge.state === `confirm` ? G.destructive : G.primaryFg }}>
              <Ico I={GitMerge} size={14} />
            </span>
            {merge.state === `merge` ? `Merge PR` : merge.state === `confirm` ? `Confirm merge` : `Merging…`}
          </div>
        ) : null}
        {stop ? (
          <div
            style={{
              height: 32,
              boxSizing: `border-box`,
              display: `flex`,
              alignItems: `center`,
              gap: 6,
              padding: `0 12px`,
              borderRadius: 999,
              border: `1px solid ${G.strokeCard}`,
              backgroundColor: G.fillCard,
              color: G.destructive,
              fontSize: T.sm,
            }}
          >
            <Ico I={CircleX} size={12} />
            Stop
          </div>
        ) : null}
        {merge ? (
          <span style={{ display: `flex`, color: G.muted, width: 24, justifyContent: `center` }}>
            <Ico I={Ellipsis} size={14} />
          </span>
        ) : null}
      </div>

      {/* the strip: + · Conversation · the issue · ┆ · Guide */}
      <div
        style={{
          position: `absolute`,
          left: HEADER_INSET,
          right: HEADER_INSET,
          top: STRIP_TOP,
          height: 32,
          display: `flex`,
          alignItems: `center`,
          gap: 6,
        }}
      >
        <span style={{ width: 32, display: `flex`, justifyContent: `center`, color: G.muted }}>
          <Ico I={Plus} size={12} />
        </span>
        <StripChip selected={face === `conversation`}>
          <Ico I={MessageCircle} size={12} />
          <span style={{ fontSize: T.sm }}>Conversation</span>
        </StripChip>
        <StripChip>
          <StatusGlyph status={issue.status} size={12} />
          <span style={{ fontFamily: MONO_FONT, fontSize: T.xs, color: G.muted }}>{issue.id}</span>
          <span
            style={{
              fontSize: T.sm,
              color: G.muted,
              maxWidth: 140,
              whiteSpace: `nowrap`,
              overflow: `hidden`,
              textOverflow: `ellipsis`,
            }}
          >
            {issue.title}
          </span>
        </StripChip>
        <span style={{ flex: 1 }} />
        <span style={{ width: 1, height: 16, backgroundColor: G.border, marginRight: 10 }} />
        <StripChip selected={face === `guide`}>
          <Ico I={Code} size={12} />
          <span style={{ fontSize: T.sm }}>Guide</span>
          <span style={{ width: 6, height: 6, borderRadius: 999, backgroundColor: `#22c55e` }} />
        </StripChip>
      </div>
      <div
        style={{
          position: `absolute`,
          left: HEADER_INSET,
          width: width - 2 * HEADER_INSET,
          top: STRIP_TOP + 32 + 14,
          height: 1,
          backgroundColor: G.border,
        }}
      />
    </>
  )
}

const StripChip: React.FC<{ selected?: boolean; children: React.ReactNode }> = ({
  selected,
  children,
}) => (
  <div
    style={{
      height: 32,
      boxSizing: `border-box`,
      display: `flex`,
      alignItems: `center`,
      gap: 6,
      padding: `0 10px`,
      borderRadius: 10,
      border: `1px solid ${selected ? G.strokeActive : G.strokeRow}`,
      backgroundColor: selected ? G.fillActive : `transparent`,
      color: selected ? G.fg : `rgba(250,250,250,0.82)`,
      whiteSpace: `nowrap`,
      flex: `none`,
    }}
  >
    {children}
  </div>
)

export const THREAD_BODY_TOP = STRIP_TOP + 32 + 14 + 1

// ── Transcript ────────────────────────────────────────────────────────────────
const TraceRow: React.FC<{ item: Extract<TranscriptItem, { kind: `trace` }> }> = ({ item }) => (
  <div style={{ height: 24, display: `flex`, alignItems: `center`, gap: 6, fontSize: T.sm }}>
    <span style={{ color: G.muted, display: `flex` }}>
      <Ico I={TRACE_ICON[item.icon]} size={13} />
    </span>
    <span style={{ color: G.fg, whiteSpace: `nowrap` }}>{item.verb}</span>
    <span style={{ color: G.muted, whiteSpace: `nowrap`, overflow: `hidden`, textOverflow: `ellipsis` }}>
      {item.detail}
    </span>
  </div>
)

const EditsCard: React.FC<{
  frame: number
  files: Extract<TranscriptItem, { kind: `edits` }>[`files`]
}> = ({ frame, files }) => {
  const shown = files.filter((f) => f.at === undefined || frame >= f.at)
  return (
    <div
      style={{
        borderRadius: 12,
        border: `1px solid ${G.strokeCard}`,
        backgroundColor: G.fillCard,
        overflow: `hidden`,
        margin: `4px 0`,
      }}
    >
      <div
        style={{
          height: 28,
          display: `flex`,
          alignItems: `center`,
          gap: 7,
          padding: `0 10.5px`,
          fontSize: T.xs,
          fontWeight: 500,
        }}
      >
        <Ico I={FileDiff} size={10.5} color="rgba(161,161,161,0.6)" />
        {shown.length === 1 ? `1 file edited` : `${shown.length} files edited`}
      </div>
      {shown.map((f) => (
        <div
          key={f.path}
          style={{
            height: 30,
            boxSizing: `border-box`,
            borderTop: `1px solid ${G.strokeRow}`,
            display: `flex`,
            alignItems: `center`,
            gap: 8,
            padding: `0 10.5px`,
            fontSize: T.xs,
          }}
        >
          <Ico I={ChevronRight} size={11} color={G.muted} />
          <span
            style={{
              flex: 1,
              minWidth: 0,
              fontFamily: MONO_FONT,
              color: G.fg,
              whiteSpace: `nowrap`,
              overflow: `hidden`,
              textOverflow: `ellipsis`,
            }}
          >
            {f.path}
          </span>
          <span style={{ fontFamily: MONO_FONT, color: `#34d399` }}>{`+${f.add}`}</span>
          <span style={{ fontFamily: MONO_FONT, color: `#fb7185` }}>{`-${f.del}`}</span>
        </div>
      ))}
    </div>
  )
}

export type TranscriptProps = {
  frame: number
  items: TranscriptItem[]
  /** global frame each item lands (same length as items) */
  schedule: readonly number[]
  width: number
  height: number
}

/** The ONE scroll region: items stack top-down in the 760 conversation
 * column; the Dock (composer) floats over its bottom. */
export const Transcript: React.FC<TranscriptProps> = ({
  frame,
  items,
  schedule,
  width,
  height,
}) => {
  const colX = (width - CONVERSATION_W) / 2 + GUTTER
  const colW = CONVERSATION_W - 2 * GUTTER
  return (
    <div
      style={{
        position: `absolute`,
        left: 0,
        top: THREAD_BODY_TOP,
        width,
        height: height - THREAD_BODY_TOP,
        overflow: `hidden`,
      }}
    >
      <div
        style={{
          position: `absolute`,
          left: colX,
          top: 24,
          width: colW,
          display: `flex`,
          flexDirection: `column`,
          gap: 8,
        }}
      >
        {items.map((item, i) => {
          const at = schedule[i] ?? 0
          if (frame < at) return null
          const t = interpolate(frame, [at, at + 8], [0, 1], { ...CLAMP, easing: EASE })
          const style: React.CSSProperties = { opacity: t, translate: `0px ${(1 - t) * 6}px` }
          switch (item.kind) {
            case `trace`:
              return (
                <div key={i} style={style}>
                  <TraceRow item={item} />
                </div>
              )
            case `prose`:
              return (
                <div key={i} style={{ ...style, display: `flex`, gap: 6, margin: `4px 0` }}>
                  <span style={{ color: G.muted, display: `flex`, paddingTop: 5 }}>
                    <Ico I={Sparkles} size={11} />
                  </span>
                  <span style={{ fontSize: T.base, lineHeight: `22px` }}>{item.text}</span>
                </div>
              )
            case `edits`:
              return (
                <div key={i} style={style}>
                  <EditsCard frame={frame} files={item.files} />
                </div>
              )
            case `user`:
              return (
                <div
                  key={i}
                  style={{
                    ...style,
                    display: `flex`,
                    flexDirection: `column`,
                    alignItems: `flex-end`,
                    gap: 6,
                    margin: `10px 0 6px`,
                  }}
                >
                  <div
                    style={{
                      maxWidth: 560,
                      boxSizing: `border-box`,
                      padding: `9px 12px`,
                      borderRadius: 12,
                      border: `1px solid ${G.strokeCard}`,
                      backgroundColor: G.fillCard,
                      fontSize: T.base,
                      lineHeight: `22px`,
                    }}
                  >
                    {item.text}
                  </div>
                  <span style={{ fontSize: 9, color: G.muted }}>{item.byline}</span>
                </div>
              )
          }
        })}
      </div>
    </div>
  )
}

// ── The composer (THE Dock's last row, always) ────────────────────────────────
export const Composer: React.FC<{
  frame: number
  width: number
  height: number
  model: string
  /** text being typed into it */
  text?: string
  /** a soft glow when a steer lands from another client */
  glowAt?: number
  /** live turn: the busy ring beside Send */
  busy?: boolean
}> = ({ frame, width, height, model, text, glowAt, busy }) => {
  const colX = (width - WORK_W) / 2 + GUTTER
  const colW = WORK_W - 2 * GUTTER
  const glow =
    glowAt === undefined
      ? 0
      : interpolate(frame, [glowAt, glowAt + 6, glowAt + 30], [0, 1, 0], CLAMP)
  return (
    <div
      style={{
        position: `absolute`,
        left: colX,
        width: colW,
        top: height - 16 - 108,
        height: 108,
        boxSizing: `border-box`,
        borderRadius: 16,
        border: `1px solid ${glow > 0 ? `rgba(255,255,255,${0.12 + 0.25 * glow})` : G.strokeCard}`,
        backgroundColor: `#1d1d20`,
        boxShadow: `0 -12px 40px rgba(17,17,20,0.9)`,
      }}
    >
      <div
        style={{
          position: `absolute`,
          left: 18,
          top: 16,
          right: 18,
          fontSize: T.sm,
          color: text ? G.fg : G.muted,
        }}
      >
        {text || `Type / for commands`}
      </div>
      <div
        style={{
          position: `absolute`,
          left: 14,
          right: 14,
          bottom: 14,
          height: 28,
          display: `flex`,
          alignItems: `center`,
          gap: 14,
          color: G.muted,
          fontSize: T.xs,
        }}
      >
        <Ico I={Plus} size={12} />
        <span style={{ display: `flex`, alignItems: `center`, gap: 6 }}>
          <Ico I={Monitor} size={12} />
          <Ico I={ChevronDown} size={11} />
        </span>
        <span style={{ display: `flex`, alignItems: `center`, gap: 6 }}>
          {model}
          <Ico I={ChevronDown} size={11} />
        </span>
        <span style={{ display: `flex`, alignItems: `center`, gap: 6 }}>
          Plan
          <span
            style={{
              width: 34,
              height: 20,
              borderRadius: 999,
              backgroundColor: `rgba(255,255,255,0.12)`,
              position: `relative`,
            }}
          >
            <span
              style={{
                position: `absolute`,
                left: 3,
                top: 3,
                width: 14,
                height: 14,
                borderRadius: 999,
                backgroundColor: `#1d1d20`,
              }}
            />
          </span>
        </span>
        <span style={{ flex: 1 }} />
        {busy ? (
          <span style={{ display: `flex`, rotate: `${frame * 14}deg`, opacity: 0.8 }}>
            <Ico I={LoaderCircle} size={11} />
          </span>
        ) : null}
        <span
          style={{
            width: 24,
            height: 24,
            boxSizing: `border-box`,
            borderRadius: 999,
            border: `1.5px solid rgba(161,161,161,0.7)`,
            display: `flex`,
            alignItems: `center`,
            justifyContent: `center`,
          }}
        >
          <Ico I={ArrowUp} size={12} />
        </span>
      </div>
    </div>
  )
}

// ── Guide › Changes (the one diff view) ───────────────────────────────────────
export type DiffLine = { t: `ctx` | `add` | `del` | `hunk`; text: string; old?: number; new?: number }
export type ChangesFile = {
  path: string
  status: `M` | `A`
  add: number
  del: number
  lines: DiffLine[]
  /** muted "N unchanged lines" lead row */
  unchangedAbove?: number
}

const KW = /\b(const|let|await|async|return|if|import|from|export|function|expect|it|describe|true|false)\b/
const tint = (text: string): React.ReactNode[] => {
  const out: React.ReactNode[] = []
  const re = /(`[^`]*`|"[^"]*"|'[^']*'|\b\d[\d_]*\b|\b(?:const|let|await|async|return|if|import|from|export|function|true|false)\b|\b[A-Za-z_]\w*(?=\())/g
  let last = 0
  let m: RegExpExecArray | null
  let k = 0
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) out.push(text.slice(last, m.index))
    const tok = m[0]
    const color = /^[`"']/.test(tok)
      ? `#a5d6ff`
      : /^\d/.test(tok)
        ? `#79c0ff`
        : KW.test(tok)
          ? `#ff7b72`
          : `#d2a8ff`
    out.push(
      <span key={k++} style={{ color }}>
        {tok}
      </span>
    )
    last = m.index + tok.length
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

const LINE_H = 20.5

export const ChangesView: React.FC<{
  frame: number
  files: ChangesFile[]
  width: number
  height: number
  /** the diff paints in from this global frame */
  paintAt?: number
}> = ({ frame, files, width, height, paintAt = 0 }) => {
  const colX = (width - WORK_W) / 2 + GUTTER - 14
  const colW = WORK_W - 2 * GUTTER + 28
  const add = files.reduce((s, f) => s + f.add, 0)
  const del = files.reduce((s, f) => s + f.del, 0)
  const treeW = 172
  const p = interpolate(frame, [paintAt, paintAt + 12], [0, 1], { ...CLAMP, easing: EASE })
  return (
    <div
      style={{
        position: `absolute`,
        left: 0,
        top: THREAD_BODY_TOP,
        width,
        height: height - THREAD_BODY_TOP,
        overflow: `hidden`,
        fontFamily: UI_FONT,
      }}
    >
      <div
        style={{
          position: `absolute`,
          left: colX,
          width: colW,
          top: 22,
          height: 26,
          display: `flex`,
          alignItems: `center`,
          gap: 14,
          fontSize: T.base,
        }}
      >
        <span style={{ display: `flex`, alignItems: `center`, gap: 8 }}>
          <Ico I={ArrowLeft} size={14} />
          Guide
        </span>
        <span style={{ width: 1, height: 16, backgroundColor: G.border }} />
        <span>Changes</span>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: T.sm, color: G.muted }}>
          <span style={{ color: `#34d399` }}>{`+${add}`}</span>{` `}
          <span style={{ color: `#fb7185` }}>{`-${del}`}</span>
          {` · ${files.length} files`}
        </span>
      </div>
      {/* the file tree */}
      <div
        style={{
          position: `absolute`,
          left: colX,
          top: 62,
          width: treeW,
          boxSizing: `border-box`,
          borderRadius: 12,
          border: `1px solid ${G.strokeRow}`,
          backgroundColor: G.fillSection,
          fontSize: T.sm,
          opacity: p,
        }}
      >
        <div style={{ height: 32, display: `flex`, alignItems: `center`, padding: `0 12px`, gap: 4 }}>
          {`${files.length} files `}
          <span style={{ color: `#34d399` }}>{`+${add}`}</span>
          <span style={{ color: `#fb7185` }}>{`-${del}`}</span>
        </div>
        <div style={{ padding: `0 8px 8px`, borderTop: `1px solid ${G.strokeRow}` }}>
          <div
            style={{
              margin: `8px 0`,
              height: 26,
              boxSizing: `border-box`,
              borderRadius: 999,
              border: `1px solid ${G.strokeCard}`,
              backgroundColor: G.fillCard,
              display: `flex`,
              alignItems: `center`,
              gap: 6,
              padding: `0 10px`,
              color: G.muted,
              fontSize: T.sm,
            }}
          >
            <Ico I={Search} size={12} />
            Filter files
          </div>
          <TreeRow depth={0} folder name="apps/shop/src/…" add={add} del={del} />
          <TreeRow depth={1} folder name="checkout" add={add} del={del} />
          {files.map((f) => (
            <TreeRow
              key={f.path}
              depth={2}
              status={f.status}
              name={f.path.split(`/`).pop() ?? f.path}
              add={f.add}
              del={f.del}
            />
          ))}
        </div>
      </div>
      {/* the diff cards */}
      <div
        style={{
          position: `absolute`,
          left: colX + treeW + 12,
          right: width - (colX + colW),
          top: 62,
          display: `flex`,
          flexDirection: `column`,
          gap: 12,
          opacity: p,
          translate: `0px ${(1 - p) * 8}px`,
        }}
      >
        {files.map((f) => (
          <div
            key={f.path}
            style={{
              borderRadius: 12,
              border: `1px solid ${G.strokeRow}`,
              backgroundColor: G.fillSection,
              overflow: `hidden`,
              fontFamily: MONO_FONT,
              fontSize: 11.5,
            }}
          >
            <div
              style={{
                height: 32,
                display: `flex`,
                alignItems: `center`,
                gap: 10,
                padding: `0 12px`,
                backgroundColor: G.fillSection,
                borderBottom: `1px solid ${G.strokeRow}`,
              }}
            >
              <span style={{ color: G.muted }}>{f.status}</span>
              <span style={{ flex: 1, minWidth: 0, color: G.muted, whiteSpace: `nowrap`, overflow: `hidden` }}>
                {f.path.slice(0, f.path.lastIndexOf(`/`) + 1)}
                <span style={{ color: G.fg }}>{f.path.split(`/`).pop()}</span>
              </span>
              <span style={{ color: `#34d399` }}>{`+${f.add}`}</span>
              <span style={{ color: `#fb7185` }}>{`-${f.del}`}</span>
              <Ico I={ChevronUp} size={12} color={G.muted} />
            </div>
            {f.unchangedAbove ? (
              <div style={{ height: LINE_H + 4, display: `flex`, alignItems: `center`, padding: `0 12px`, color: `rgba(161,161,161,0.6)` }}>
                {`${f.unchangedAbove} unchanged lines`}
              </div>
            ) : null}
            {f.lines.map((l, i) =>
              l.t === `hunk` ? (
                <div
                  key={i}
                  style={{
                    height: LINE_H + 4,
                    display: `flex`,
                    alignItems: `center`,
                    padding: `0 12px`,
                    color: G.muted,
                    backgroundColor: `rgba(255,255,255,0.04)`,
                    whiteSpace: `nowrap`,
                  }}
                >
                  {l.text}
                </div>
              ) : (
                <div
                  key={i}
                  style={{
                    height: LINE_H,
                    display: `flex`,
                    alignItems: `center`,
                    whiteSpace: `pre`,
                    backgroundColor:
                      l.t === `add`
                        ? `rgba(16,185,129,0.10)`
                        : l.t === `del`
                          ? `rgba(244,63,94,0.10)`
                          : undefined,
                  }}
                >
                  <span style={{ width: 40, textAlign: `right`, color: `rgba(161,161,161,0.5)`, flex: `none` }}>
                    {l.old ?? ``}
                  </span>
                  <span style={{ width: 40, textAlign: `right`, color: `rgba(161,161,161,0.5)`, flex: `none` }}>
                    {l.new ?? ``}
                  </span>
                  <span
                    style={{
                      width: 26,
                      textAlign: `center`,
                      flex: `none`,
                      color: l.t === `add` ? `#34d399` : l.t === `del` ? `#fb7185` : `transparent`,
                    }}
                  >
                    {l.t === `add` ? `+` : l.t === `del` ? `-` : ` `}
                  </span>
                  <span style={{ color: `#e6edf3`, overflow: `hidden` }}>{tint(l.text)}</span>
                </div>
              )
            )}
          </div>
        ))}
      </div>
    </div>
  )
}

const TreeRow: React.FC<{
  depth: number
  name: string
  add: number
  del: number
  folder?: boolean
  status?: string
}> = ({ depth, name, add, del, folder, status }) => (
  <div
    style={{
      height: 26,
      display: `flex`,
      alignItems: `center`,
      gap: 6,
      paddingLeft: 4 + depth * 10,
      fontFamily: MONO_FONT,
      fontSize: 11,
      whiteSpace: `nowrap`,
    }}
  >
    {folder ? (
      <Ico I={Folder} size={11} color={G.muted} />
    ) : (
      <span style={{ color: G.muted, width: 11, textAlign: `center` }}>{status}</span>
    )}
    <span style={{ flex: 1, minWidth: 0, overflow: `hidden`, textOverflow: `ellipsis` }}>{name}</span>
    <span style={{ color: `#34d399` }}>{`+${add}`}</span>
    <span style={{ color: `#fb7185` }}>{`-${del}`}</span>
    {folder ? <Ico I={ChevronUp} size={10} color={G.muted} /> : <span style={{ width: 10 }} />}
  </div>
)
