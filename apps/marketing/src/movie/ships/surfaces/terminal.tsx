// surfaces/terminal.tsx — TerminalDock: the bottom dock hosting the embedded Claude Code
// CLI session, rendered with the REAL CLI grammar (● Tool(args) / "⎿ result" continuations /
// ✳ spinner with live counters / "❯" prompt between hairline rules / bypass-permissions footer).
// Pixel truth: the desktop-claude-session-dock reference screenshot (local-only, untracked; plus the inline numbered diff
// lines a tool event can carry). Frames are composition-global via props.
// EXP-723 (terminal_dock.rs): the dock lives INSIDE the cutout panel on the opaque
// popover under a card hairline, with its own 28px header row on top (Open in new
// window · the collapsed-form switch (EXP-742) · Hide terminal, right-aligned) and
// the tabs strip — rich-tab chips + the `+`, nothing else — pinned along the bottom.
// The same strip IS the collapsed dock (`DockStrip`).

import React from "react"
import { interpolate, spring } from "remotion"
import type { SessionEvent } from "../fixtures"
import { rollNum, typed, useBlink } from "../rig"
import { C, EASE, MONO_FONT, POP, UI_FONT, WIN } from "../theme"

// ── metrics (window-local px; ref proportions rescaled to the 1568-wide window) ──
const FS = 12 // terminal grid font size (contract: JetBrains Mono 12px/1.45)
const PADX = 14 // grid horizontal padding
const GUTTER = 18 // dot/glyph column width (keeps continuation indent mono-stable)
const INPUT_H = 30 // "❯" prompt row between the two hairline rules
const FOOTER_H = 26 // bypass-permissions status row
const BOTTOM_H = INPUT_H + FOOTER_H
const EXIT_H = 28 // "Process finished with exit code 0" strip
const BLOCK_GAP = 14 // blank-line rhythm between CLI blocks
const TYPE_CPF = 2 // tool args typing speed (storyboard §0: 2 chars/frame)

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

const SPIN_GLYPHS = [`✳`, `✶`, `✻`, `✽`] // the CLI's cycling spinner glyph, frame-driven
const DEFAULT_SPINNER_TIP = `Tip: Use /btw to ask a quick side question without interrupting the agent's current work`
const DEFAULT_SPINNER_BASE = { sec: 161, tokensK: 12.3 } // "2m 41s · ↓ 12.3k tokens"

// ── tiny inline icons (lucide-style, currentColor) ────────────────────────────
const IcX: React.FC<{ size?: number }> = ({ size = 10 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
)
const IcPlus: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
    <path d="M12 5v14M5 12h14" />
  </svg>
)
const IcChevronDown: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="m6 9 6 6 6-6" />
  </svg>
)
// ui-undock = arrow-up-right, the EXP-742 switch = message-circle,
// session-shell = terminal, nav-terminal = square-terminal (icons.json).
const IcArrowUpRight: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M7 7h10v10" />
    <path d="M7 17 17 7" />
  </svg>
)
const IcMessageCircle: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
  </svg>
)
const IcTerminal: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="m4 17 6-6-6-6" />
    <path d="M12 19h8" />
  </svg>
)
const IcSquareTerminal: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} strokeLinecap="round" strokeLinejoin="round">
    <path d="m7 11 2-2-2-2" />
    <path d="M11 13h4" />
    <rect x="3" y="3" width="18" height="18" rx="2" />
  </svg>
)
const IcCheck: React.FC = () => (
  <svg
    width={11}
    height={11}
    viewBox="0 0 24 24"
    fill="none"
    stroke={C.green}
    strokeWidth={3}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ marginTop: 3 }}
  >
    <path d="M20 6 9 17l-5-5" />
  </svg>
)

// ── grid line primitives ──────────────────────────────────────────────────────
const revealStyle = (frame: number, at: number): React.CSSProperties => ({
  opacity: interpolate(frame, [at, at + 3], [0, 1], EASED),
  translate: `0px ${interpolate(frame, [at, at + 3], [4, 0], EASED)}px`,
})

const Line: React.FC<{
  frame: number
  at: number
  gapTop?: boolean
  bg?: string
  gutter?: React.ReactNode
  children: React.ReactNode
}> = ({ frame, at, gapTop = false, bg, gutter, children }) => {
  if (frame < at) return null
  return (
    <div
      style={{
        display: `flex`,
        padding: `0 ${PADX}px`,
        marginTop: gapTop ? BLOCK_GAP : 0,
        backgroundColor: bg,
        ...revealStyle(frame, at),
      }}
    >
      <div style={{ width: GUTTER, flexShrink: 0 }}>{gutter}</div>
      <div style={{ flex: 1, minWidth: 0, whiteSpace: `pre-wrap`, overflowWrap: `anywhere` }}>{children}</div>
    </div>
  )
}

const GlyphDot: React.FC<{ color: string }> = ({ color }) => (
  <span style={{ display: `inline-block`, width: 7, height: 7, borderRadius: 999, backgroundColor: color, marginTop: 5 }} />
)

// Optional inline-diff extras on tool events:
// a line "792 -   run_sync_worker(…)" renders as numbered gutter + red/green tinted band.
const DIFF_EXTRA = /^(\d+) ([+-])? ?(.*)$/
const rightPad4 = (s: string) => `${`    `.slice(s.length)}${s}` // no String.padStart in lib es2015

const ExtraLine: React.FC<{ frame: number; at: number; text: string }> = ({ frame, at, text }) => {
  const m = DIFF_EXTRA.exec(text)
  if (!m) {
    return (
      <Line frame={frame} at={at}>
        <span style={{ color: C.dim }}>{text}</span>
      </Line>
    )
  }
  const sign = m[2]
  const tone = sign === `+` ? C.diffAdd : sign === `-` ? C.diffDel : C.dim
  const bg = sign === `+` ? C.diffAddBg : sign === `-` ? C.diffDelBg : undefined
  const content = m[3]
  return (
    <Line frame={frame} at={at} bg={bg}>
      <span style={{ color: tone }}>{`${rightPad4(m[1])} ${sign ?? ` `}  `}</span>
      <span style={{ color: /^\s*\/\//.test(content) ? C.synComment : `#d4d4d4` }}>{content}</span>
    </Line>
  )
}

// ── event blocks (real Claude Code CLI grammar) ───────────────────────────────
const ToolBlock: React.FC<{ frame: number; at: number; ev: Extract<SessionEvent, { kind: "tool" }> }> = ({
  frame,
  at,
  ev,
}) => {
  const args = ev.args ?? ``
  const argFrames = args ? Math.ceil(args.length / TYPE_CPF) : 0
  const shown = typed(args, frame, at, TYPE_CPF)
  const argsDone = frame >= at + argFrames
  const resultAt = at + argFrames + 3
  return (
    <>
      <Line frame={frame} at={at} gapTop gutter={<GlyphDot color={C.termToolDot} />}>
        <span style={{ color: C.text, fontWeight: 700 }}>{ev.tool}</span>
        {args ? <span style={{ color: `#d4d4d4` }}>{`(${shown}${argsDone ? `)` : ``}`}</span> : null}
      </Line>
      {ev.result !== undefined ? (
        <Line frame={frame} at={resultAt}>
          <span style={{ color: C.muted }}>{`⎿  ${ev.result}`}</span>
        </Line>
      ) : null}
      {(ev.extra ?? []).map((x, j) => (
        <ExtraLine key={j} frame={frame} at={resultAt + 2 * (j + 1)} text={x} />
      ))}
    </>
  )
}

const ProseBlock: React.FC<{ frame: number; at: number; text: string }> = ({ frame, at, text }) => (
  <Line frame={frame} at={at} gapTop gutter={<GlyphDot color={C.termProseDot} />}>
    <span style={{ color: `#d4d4d4` }}>{text}</span>
  </Line>
)

const FlashBlock: React.FC<{ frame: number; at: number; text: string }> = ({ frame, at, text }) => (
  <Line
    frame={frame}
    at={at}
    gapTop
    bg={`rgba(34,197,94,${interpolate(frame, [at, at + 8], [0.2, 0], CLAMP)})`}
    gutter={<IcCheck />}
  >
    <span style={{ color: C.text }}>{text}</span>
  </Line>
)

const SpinnerBlock: React.FC<{
  frame: number
  at: number
  verb: string
  baseSec: number
  baseTokensK: number
  tip: string | null
}> = ({ frame, at, verb, baseSec, baseTokensK, tip }) => {
  const glyph = SPIN_GLYPHS[Math.floor(Math.max(0, frame - at) / 9) % SPIN_GLYPHS.length]
  // Live counters (MUST tick): +1s every 30f, +0.1k tokens every 6f.
  const sec = baseSec + rollNum(frame, at, at + 3600, 0, 120)
  const tokTenths = Math.round(baseTokensK * 10) + rollNum(frame, at, at + 3600, 0, 600)
  const elapsed = `${Math.floor(sec / 60)}m ${sec % 60}s`
  const tokens = `${(tokTenths / 10).toFixed(1)}k`
  return (
    <>
      <Line frame={frame} at={at} gapTop gutter={<span style={{ color: C.termSpinner }}>{glyph}</span>}>
        <span style={{ color: C.termSpinner, fontWeight: 700 }}>{`${verb}… `}</span>
        <span style={{ color: C.muted }}>{`(${elapsed} · ↓ ${tokens} tokens)`}</span>
      </Line>
      {tip !== null ? (
        <Line frame={frame} at={at + 2}>
          <span style={{ color: C.dim }}>{`⎿  ${tip}`}</span>
        </Line>
      ) : null}
    </>
  )
}

// ── dock tabs ─────────────────────────────────────────────────────────────────
// surface::rich_tab — a session chip leads with its status dot and mono
// identifier; a shell chip leads with the `session-shell` glyph and is named
// by its cwd.
export type DockTab = {
  id: string
  label: string
  dot?: string // leading status dot color (running session); omit for plain tabs
  identifier?: string // mono issue identifier ahead of the title (a session chip)
  shell?: boolean // a plain shell tab: the terminal glyph leads instead of a dot
  popAt?: number // global frame the tab POP-springs in; omit = present from frame 0
}

const TabItem: React.FC<{
  frame: number
  tab: DockTab
  active: boolean
  badgeAt?: number // global frame the green `0` exit badge pops on this tab
}> = ({ frame, tab, active, badgeAt }) => {
  if (tab.popAt !== undefined && frame < tab.popAt) return null
  const popT = tab.popAt === undefined ? 1 : spring({ frame: frame - tab.popAt, fps: 30, config: POP })
  const badgeOn = badgeAt !== undefined && frame >= badgeAt
  return (
    <div
      style={{
        display: `flex`,
        alignItems: `center`,
        alignSelf: `center`,
        height: 24,
        boxSizing: `border-box`,
        gap: 7,
        padding: `0 10px`,
        marginRight: 4,
        borderRadius: 10,
        backgroundColor: active ? C.fillActive : `transparent`,
        scale: String(0.8 + 0.2 * popT),
        opacity: Math.min(1, popT * 2),
        transformOrigin: `center bottom`,
      }}
    >
      {tab.shell ? (
        <span style={{ color: active ? C.text : C.muted, display: `flex`, flexShrink: 0 }}>
          <IcTerminal size={12} />
        </span>
      ) : null}
      {tab.dot !== undefined ? (
        <span style={{ width: 5, height: 5, borderRadius: 999, backgroundColor: tab.dot, flexShrink: 0 }} />
      ) : null}
      {tab.identifier !== undefined ? (
        <span style={{ fontFamily: MONO_FONT, fontSize: 11.5, color: C.muted, whiteSpace: `nowrap` }}>
          {tab.identifier}
        </span>
      ) : null}
      <span
        style={{
          fontFamily: UI_FONT,
          fontSize: 13,
          fontWeight: active ? 500 : 400,
          color: active ? C.text : C.muted,
          whiteSpace: `nowrap`,
          maxWidth: 180,
          overflow: `hidden`,
          textOverflow: `ellipsis`,
        }}
      >
        {tab.label}
      </span>
      {badgeOn ? (
        <span
          style={{
            display: `inline-block`,
            fontFamily: MONO_FONT,
            fontSize: 10,
            lineHeight: 1,
            color: C.green,
            backgroundColor: `rgba(34,197,94,0.15)`,
            borderRadius: 3,
            padding: `2px 4px`,
            scale: String(spring({ frame: frame - (badgeAt ?? 0), fps: 30, config: POP })),
          }}
        >
          0
        </span>
      ) : null}
      <span style={{ color: C.dim, display: `flex` }}>
        <IcX />
      </span>
    </div>
  )
}

// ── the strip (render_strip, EXP-688/723) ─────────────────────────────────────
// ONE 29px band pinned to the panel's bottom edge, open or collapsed: the
// rich-tab chips with the `+` right after them. An EMPTY strip names itself
// (the terminal glyph + "Terminal"). Card hairline on top, FILL_CARD wash —
// over the panel when collapsed, over the popover when the dock is open.
export const DockStrip: React.FC<{
  frame: number
  tabs: DockTab[]
  activeTab?: string
  exitAt?: number
  exitBadgeTab?: string
}> = ({ frame, tabs, activeTab, exitAt, exitBadgeTab }) => {
  const shown = tabs.filter((t) => t.popAt === undefined || frame >= t.popAt)
  return (
    <div
      style={{
        position: `absolute`,
        left: 0,
        right: 0,
        bottom: 0,
        height: WIN.dockStrip,
        boxSizing: `border-box`,
        borderTop: `1px solid ${C.strokeCard}`,
        backgroundColor: C.fillCard,
        display: `flex`,
        alignItems: `center`,
        gap: 3.5,
        padding: `0 7px`,
        fontFamily: UI_FONT,
      }}
    >
      {shown.length === 0 ? (
        <span style={{ display: `flex`, alignItems: `center`, gap: 5, color: C.muted, flexShrink: 0 }}>
          <IcSquareTerminal size={12} />
          <span style={{ fontSize: 11.5 }}>Terminal</span>
        </span>
      ) : (
        tabs.map((t) => (
          <TabItem
            key={t.id}
            frame={frame}
            tab={t}
            active={t.id === activeTab}
            badgeAt={t.id === (exitBadgeTab ?? activeTab) ? exitAt : undefined}
          />
        ))
      )}
      {/* the `+` rides the slot right AFTER the last tab (JetBrains placement) */}
      <div
        style={{
          width: 20,
          height: 20,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `center`,
          color: C.muted,
          flexShrink: 0,
        }}
      >
        <IcPlus />
      </div>
    </div>
  )
}

// ── the dock ──────────────────────────────────────────────────────────────────
export type TerminalFeed = {
  events: readonly SessionEvent[]
  schedule: readonly number[] // schedule[i] = global frame event i reveals (3f fade + 4px rise)
}

export const TerminalDock: React.FC<{
  frame: number
  height: number // animated by the assembler (29 → 240; content fades in as it grows)
  tabs: DockTab[]
  activeTab: string
  feed: TerminalFeed
  inputGlow?: number // global frame: soft white pulse on the prompt box (remote-steer beat)
  exitAt?: number // global frame: exit strip fades in, input/footer fade out, tab gets green `0`
  exitBadgeTab?: string // tab receiving the exit badge (defaults to activeTab)
  inputText?: { text: string; at: number; cpf?: number } // optional typed text after "❯ "
  spinnerBase?: { sec: number; tokensK: number } // counter start values ("2m 41s · ↓ 12.3k tokens")
  spinnerTip?: string | null // muted "⎿ Tip: …" under the spinner; null hides it
}> = ({
  frame,
  height,
  tabs,
  activeTab,
  feed,
  inputGlow,
  exitAt,
  exitBadgeTab,
  inputText,
  spinnerBase = DEFAULT_SPINNER_BASE,
  spinnerTip,
}) => {
  const h = Math.max(0, height)
  const blinkOn = useBlink(frame)
  const contentO = interpolate(h, [60, 180], [0, 1], CLAMP)
  const inputO = exitAt === undefined ? 1 : interpolate(frame, [exitAt, exitAt + 6], [1, 0], CLAMP)
  const glowT =
    inputGlow === undefined ? 0 : interpolate(frame, [inputGlow, inputGlow + 8, inputGlow + 40], [0, 1, 0], EASED)
  const typing = inputText !== undefined && frame >= inputText.at
  const typedInput = typing ? typed(inputText.text, frame, inputText.at, inputText.cpf ?? 1) : ``
  const stillTyping = typing && typedInput.length < inputText.text.length
  const tip = spinnerTip === undefined ? DEFAULT_SPINNER_TIP : spinnerTip
  const badgeTab = exitBadgeTab ?? activeTab

  const tool: React.CSSProperties = {
    width: 20,
    height: 20,
    display: `flex`,
    alignItems: `center`,
    justifyContent: `center`,
    color: C.muted,
    borderRadius: 6,
  }

  return (
    <div
      style={{
        position: `absolute`,
        left: WIN.panel.x,
        right: WIN.w - WIN.panel.right,
        bottom: WIN.h - WIN.panel.bottom,
        height: h,
        backgroundColor: C.popover,
        borderTop: `1px solid ${C.strokeCard}`,
        overflow: `hidden`,
      }}
    >
      {/* render_dock_header (EXP-723): the window controls, right-aligned —
          undock the ACTIVE tab, EXP-742's strip ⇄ bubble switch, collapse */}
      <div
        style={{
          position: `absolute`,
          top: 0,
          left: 0,
          right: 0,
          height: WIN.dockHeader,
          boxSizing: `border-box`,
          borderBottom: `1px solid ${C.strokeRow}`,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `flex-end`,
          gap: 1.75,
          padding: `0 7px`,
          opacity: contentO,
        }}
      >
        <div style={tool}>
          <IcArrowUpRight />
        </div>
        <div style={tool}>
          <IcMessageCircle />
        </div>
        <div style={tool}>
          <IcChevronDown />
        </div>
      </div>

      {/* terminal grid — bottom-aligned feed, newest line directly above the prompt */}
      <div
        style={{
          position: `absolute`,
          top: WIN.dockHeader,
          left: 0,
          right: 0,
          bottom: BOTTOM_H + WIN.dockStrip,
          overflow: `hidden`,
          display: `flex`,
          flexDirection: `column`,
          justifyContent: `flex-end`,
          paddingBottom: 8,
          fontFamily: MONO_FONT,
          fontSize: FS,
          lineHeight: 1.45,
          opacity: contentO,
        }}
      >
        {feed.events.map((ev, i) => {
          const at = feed.schedule[i] ?? 0
          if (frame < at) return null
          if (ev.kind === `spinner`) {
            // the CLI spinner is transient — it clears when the next block lands
            const nextAt = feed.schedule[i + 1] ?? Number.POSITIVE_INFINITY
            if (frame >= nextAt) return null
            return (
              <SpinnerBlock
                key={i}
                frame={frame}
                at={at}
                verb={ev.verb}
                baseSec={spinnerBase.sec}
                baseTokensK={spinnerBase.tokensK}
                tip={tip}
              />
            )
          }
          if (ev.kind === `tool`) return <ToolBlock key={i} frame={frame} at={at} ev={ev} />
          if (ev.kind === `prose`) return <ProseBlock key={i} frame={frame} at={at} text={ev.text} />
          return <FlashBlock key={i} frame={frame} at={at} text={ev.text} />
        })}
      </div>

      {/* prompt box between hairline rules + bypass-permissions footer */}
      <div style={{ position: `absolute`, left: 0, right: 0, bottom: WIN.dockStrip, height: BOTTOM_H, opacity: contentO * inputO }}>
        <div
          style={{
            height: INPUT_H,
            display: `flex`,
            alignItems: `center`,
            padding: `0 ${PADX}px`,
            borderTop: `1px solid ${C.strokeStrong}`,
            borderBottom: `1px solid ${C.strokeStrong}`,
            backgroundColor: `rgba(255,255,255,${0.08 * glowT})`,
            boxShadow: `0 0 ${16 * glowT}px rgba(255,255,255,${0.3 * glowT})`,
            fontFamily: MONO_FONT,
            fontSize: FS,
          }}
        >
          <span style={{ color: C.muted }}>{`❯ `}</span>
          {typedInput !== `` ? <span style={{ color: C.text }}>{typedInput}</span> : null}
          <span
            style={{
              display: `inline-block`,
              width: 7,
              height: 14,
              marginLeft: 1,
              backgroundColor: C.text,
              opacity: stillTyping || blinkOn ? 1 : 0,
            }}
          />
        </div>
        <div
          style={{
            height: FOOTER_H,
            display: `flex`,
            alignItems: `center`,
            padding: `0 ${PADX}px`,
            fontFamily: MONO_FONT,
            fontSize: FS,
            whiteSpace: `pre`,
          }}
        >
          <span style={{ color: C.termBypass }}>{`▶▶ bypass permissions on `}</span>
          <span style={{ color: C.dim }}>{`(shift+tab to cycle) · esc to interrupt · ⏎ for agents`}</span>
        </div>
      </div>

      {/* exit strip — the session process ended */}
      {exitAt !== undefined && frame >= exitAt ? (
        <div
          style={{
            position: `absolute`,
            left: 0,
            right: 0,
            bottom: WIN.dockStrip,
            height: EXIT_H,
            borderTop: `1px solid ${C.strokeRow}`,
            backgroundColor: C.popover,
            display: `flex`,
            alignItems: `center`,
            gap: 8,
            padding: `0 ${PADX}px`,
            ...revealStyle(frame, exitAt + 2),
          }}
        >
          <span style={{ width: 6, height: 6, borderRadius: 999, backgroundColor: C.green }} />
          <span style={{ fontFamily: UI_FONT, fontSize: 12, color: C.muted }}>Process finished with exit code 0</span>
        </div>
      ) : null}

      {/* the tabs strip — pinned to the dock's (= the panel's) bottom edge */}
      <DockStrip frame={frame} tabs={tabs} activeTab={activeTab} exitAt={exitAt} exitBadgeTab={badgeTab} />
    </div>
  )
}
