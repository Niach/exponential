// surfaces/dialogs.tsx — DialogScrim + the two "Start coding" dialogs (S5 issue,
// S9 release). Pixel truth: ref/desktop-release-detail-dialog.png. Measured there:
//   · the dialog card bg is the APP bg #0a0a0a (sampled (10,10,10) across the card
//     interior — not the #171717 popover tone; selects read (21,21,21) = bg + 4%
//     white fill), border ≈ white@10, radius 8, black@50 scrim behind
//   · title 15px/600 white, × top-right muted
//   · intro / select labels / checklist right-labels / captions ≈ C.muted 12px
//   · checklist: 16px rounded checkbox, mono muted identifier, 13px white title,
//     trailing muted right-label
//   · Effort select is DISABLED while ultracode is on (≈55% opacity, muted value,
//     "ultracode sets effort" caption below)
//   · "Dynamic workflows (ultracode)" toggle ON = light #e5e5e5 track + DARK thumb
//   · footer: left muted validation note, outline Cancel, primary Start coding
// Both dialogs are absolutely-positioned window-local boxes centered in the
// 1568×980 WIN; every clickable anchor is exported for cursor choreography.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, SETTLE, UI_FONT, WIN } from "../theme"
import { BOARD, HERO, IDENTITY, RELEASE, type IssueStatus } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

// 0→amt→0 pulse starting at `at` (0 when unscheduled / outside the window).
const pulse = (frame: number, at: number | undefined, up = 3, down = 9, amt = 1) =>
  at === undefined ? 0 : interpolate(frame, [at, at + up, at + down], [0, amt, 0], CLAMP)

// ── Shared option lists (real desktop launcher values) ───────────────────────
export const MODEL_OPTIONS = ["Fable", "Opus", "Sonnet"] as const
export const EFFORT_OPTIONS = ["CLI default", "Low", "Medium", "High", "XHigh", "Max"] as const

export type SelectMenuSpec = {
  openAt: number
  closeAt: number
  options: readonly string[]
  // hover highlight lands on this option; confirm (check pulse + value commit) at `at`
  highlight?: { option: string; at: number }
}

export type DialogButtonState = {
  hoverAt?: number // primary button hover-brighten
  startingAt?: number // label → "Starting…" + spinner
}

// ── Tiny inline icons (lucide-style, currentColor) ───────────────────────────
const XIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M18 6 6 18" />
    <path d="m6 6 12 12" />
  </svg>
)

const ChevronDownIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="m6 9 6 6 6-6" />
  </svg>
)

// Check with a draw parameter (dash reveal). drawT 1 = fully drawn.
const CheckGlyph: React.FC<{ size: number; stroke: string; drawT?: number; strokeWidth?: number }> = ({ size, stroke, drawT = 1, strokeWidth = 2.6 }) => {
  const LEN = 24 // safe overestimate of the path length in a 24-box
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={stroke} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round">
      <path d="M4.5 12.5 10 18 19.5 6.5" strokeDasharray={LEN} strokeDashoffset={LEN * (1 - Math.min(1, Math.max(0, drawT)))} />
    </svg>
  )
}

const Spinner: React.FC<{ frame: number; start: number; size?: number }> = ({ frame, start, size = 12 }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    style={{ rotate: `${((Math.max(0, frame - start) * 30) % 360)}deg` }}
  >
    <circle cx={8} cy={8} r={6} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeDasharray="26 12" />
  </svg>
)

// ── Scrim ─────────────────────────────────────────────────────────────────────
export const DialogScrim: React.FC<{ frame: number; in: number; out: number }> = ({ frame, in: fin, out }) => {
  if (frame < fin || frame > out + 8) return null
  const o =
    interpolate(frame, [fin, fin + 8], [0, 1], CLAMP) *
    (1 - interpolate(frame, [out, out + 8], [0, 1], CLAMP))
  if (o <= 0) return null
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        backgroundColor: "rgba(0,0,0,0.5)",
        opacity: o,
        zIndex: 40,
      }}
    />
  )
}

// ── Shared control primitives ─────────────────────────────────────────────────
export const CheckBox: React.FC<{
  frame: number
  x: number
  y: number
  checked: boolean // resting checked state
  checkAt?: number // becomes checked here (pop + check draw)
  pulseAt?: number // shimmer pulse of an already-checked box
  size?: number
}> = ({ frame, x, y, checked, checkAt, pulseAt, size = 16 }) => {
  const isChecked = checked || (checkAt !== undefined && frame >= checkAt)
  const drawT = !isChecked ? 0 : checkAt === undefined ? 1 : interpolate(frame, [checkAt, checkAt + 6], [0, 1], EASED)
  const scale = 1 + 0.18 * pulse(frame, checkAt, 3, 8) + 0.22 * pulse(frame, pulseAt, 3, 7)
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: y,
        width: size,
        height: size,
        borderRadius: 4,
        border: `1px solid ${isChecked ? C.primary : C.input}`,
        backgroundColor: isChecked ? C.primary : "rgba(255,255,255,0.03)",
        scale: String(scale),
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {isChecked ? <CheckGlyph size={size - 4} stroke={C.primaryFg} drawT={drawT} /> : null}
    </div>
  )
}

// Labeled select (label 12px/500 muted above a h30 bordered box).
export const SelectBox: React.FC<{
  frame: number
  x: number
  boxY: number // top of the box; label renders at boxY − 20
  w: number
  label: string
  value: string
  disabled?: boolean
  flickAt?: number // brief glow + value flick (release cascade)
}> = ({ frame, x, boxY, w, label, value, disabled = false, flickAt }) => {
  const glow = pulse(frame, flickAt, 3, 12)
  const dip = flickAt === undefined ? 0 : pulse(frame, flickAt, 2, 8, 0.7)
  return (
    <>
      <div
        style={{
          position: "absolute",
          left: x,
          top: boxY - 20,
          fontSize: 12,
          fontWeight: 500,
          color: C.muted,
          lineHeight: "16px",
        }}
      >
        {label}
      </div>
      <div
        style={{
          position: "absolute",
          left: x,
          top: boxY,
          width: w,
          height: 30,
          borderRadius: 6,
          border: `1px solid ${C.input}`,
          backgroundColor: "rgba(255,255,255,0.04)",
          boxShadow: glow > 0 ? `0 0 0 1px rgba(255,255,255,${0.25 * glow})` : undefined,
          opacity: disabled ? 0.55 : 1,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          paddingLeft: 10,
          paddingRight: 8,
          boxSizing: "border-box",
        }}
      >
        <span style={{ fontSize: 13, color: disabled ? C.muted : C.text, opacity: 1 - dip }}>{value}</span>
        <span style={{ color: C.muted, display: "flex" }}>
          <ChevronDownIcon />
        </span>
      </div>
    </>
  )
}

// Select popover menu (#171717 popover surface, check on the selected option).
const MENU_ROW_H = 26
const MENU_PAD = 4
const SelectMenu: React.FC<{
  frame: number
  x: number
  y: number
  w: number
  spec: SelectMenuSpec
  initial: string
}> = ({ frame, x, y, w, spec, initial }) => {
  if (frame < spec.openAt || frame > spec.closeAt + 4) return null
  const o =
    interpolate(frame, [spec.openAt, spec.openAt + 5], [0, 1], CLAMP) *
    (1 - interpolate(frame, [spec.closeAt, spec.closeAt + 4], [0, 1], CLAMP))
  if (o <= 0) return null
  const sc = interpolate(frame, [spec.openAt, spec.openAt + 6], [0.97, 1], EASED)
  const selected = spec.highlight !== undefined && frame >= spec.highlight.at ? spec.highlight.option : initial
  const checkPulse = 1 + 0.3 * pulse(frame, spec.highlight?.at, 3, 7)
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: y,
        width: w,
        borderRadius: 6,
        border: `1px solid ${C.border}`,
        backgroundColor: C.panel,
        boxShadow: "0 8px 24px rgba(0,0,0,0.5)",
        padding: MENU_PAD,
        boxSizing: "border-box",
        opacity: o,
        scale: String(sc),
        transformOrigin: "50% 0%",
        zIndex: 20,
      }}
    >
      {spec.options.map((opt) => {
        const hovered =
          spec.highlight !== undefined && opt === spec.highlight.option && frame >= spec.highlight.at - 6
        return (
          <div
            key={opt}
            style={{
              height: MENU_ROW_H,
              borderRadius: 4,
              backgroundColor: hovered ? C.accentBg : "transparent",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              paddingLeft: 8,
              paddingRight: 8,
            }}
          >
            <span style={{ fontSize: 13, color: C.text }}>{opt}</span>
            {opt === selected ? (
              <span style={{ color: C.text, display: "flex", scale: String(checkPulse) }}>
                <CheckGlyph size={12} stroke={C.text} strokeWidth={2.2} />
              </span>
            ) : null}
          </div>
        )
      })}
    </div>
  )
}

// Footer button pair (outline Cancel + primary Start coding → Starting…).
export const CANCEL_W = 72
export const PRIMARY_W = 108
export const BTN_H = 28
export const FooterButtons: React.FC<{
  frame: number
  y: number
  rightEdge: number // dialog-local right edge of the primary button
  state?: DialogButtonState
  dim?: boolean // invalid form → disabled-look primary
}> = ({ frame, y, rightEdge, state, dim = false }) => {
  const hoverT = state?.hoverAt === undefined ? 0 : interpolate(frame, [state.hoverAt, state.hoverAt + 4], [0, 1], CLAMP)
  const ch = Math.round(229 + 26 * hoverT) // #e5e5e5 → #ffffff
  const starting = state?.startingAt !== undefined && frame >= state.startingAt
  const btn: React.CSSProperties = {
    position: "absolute",
    top: y,
    height: BTN_H,
    borderRadius: 6,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    fontSize: 13,
    fontWeight: 500,
    boxSizing: "border-box",
  }
  return (
    <>
      <div
        style={{
          ...btn,
          left: rightEdge - PRIMARY_W - 8 - CANCEL_W,
          width: CANCEL_W,
          border: `1px solid ${C.input}`,
          backgroundColor: "rgba(255,255,255,0.02)",
          color: C.text,
        }}
      >
        Cancel
      </div>
      <div
        style={{
          ...btn,
          left: rightEdge - PRIMARY_W,
          width: PRIMARY_W,
          backgroundColor: `rgb(${ch},${ch},${ch})`,
          color: C.primaryFg,
          opacity: dim ? 0.5 : 1,
        }}
      >
        {starting && state?.startingAt !== undefined ? (
          <>
            <Spinner frame={frame} start={state.startingAt} />
            Starting…
          </>
        ) : (
          "Start coding"
        )}
      </div>
    </>
  )
}

// ── Dialog shell (card + title + ×, appear spring, collapse toward the dock) ──
export const DialogShell: React.FC<{
  frame: number
  appearAt: number
  collapseAt?: number
  x: number
  y: number
  w: number
  h: number
  title: string
  children: React.ReactNode
}> = ({ frame, appearAt, collapseAt, x, y, w, h, title, children }) => {
  if (frame < appearAt) return null
  const ct = collapseAt === undefined ? 0 : interpolate(frame, [collapseAt, collapseAt + 9], [0, 1], EASED)
  if (ct >= 1) return null
  const spr = spring({ frame: frame - appearAt, fps: 30, config: SETTLE })
  const scale = (0.96 + 0.04 * spr) * (1 - 0.1 * ct)
  const opacity = interpolate(frame, [appearAt, appearAt + 8], [0, 1], CLAMP) * (1 - ct)
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: y,
        width: w,
        height: h,
        borderRadius: 8,
        border: `1px solid ${C.border}`,
        backgroundColor: C.bg, // ref pixel truth: card bg = app bg, not #171717
        boxShadow: "0 24px 64px rgba(0,0,0,0.55), 0 4px 16px rgba(0,0,0,0.4)",
        opacity,
        scale: String(scale),
        translate: `0px ${80 * ct}px`,
        transformOrigin: "50% 60%",
        fontFamily: UI_FONT,
        zIndex: 50,
      }}
    >
      <div
        style={{
          position: "absolute",
          left: 20,
          top: 16,
          fontSize: 15,
          fontWeight: 600,
          letterSpacing: -0.1,
          color: C.text,
          lineHeight: "20px",
        }}
      >
        {title}
      </div>
      <div
        style={{
          position: "absolute",
          right: 14,
          top: 14,
          width: 20,
          height: 20,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: C.muted,
        }}
      >
        <XIcon />
      </div>
      {children}
    </div>
  )
}

export const captionStyle = (x: number, y: number, w: number): React.CSSProperties => ({
  position: "absolute",
  left: x,
  top: y,
  width: w,
  fontSize: 12,
  lineHeight: "18px",
  color: C.muted,
})

export const PLAN_CAPTION = `Present a plan for approval before making changes (native Claude plan mode). After approving, Shift+Tab switches to skip-permissions for a prompt-free run.`

// ── Issue variant (420px, S5) ─────────────────────────────────────────────────
const I = {
  w: 420,
  h: 310,
  pad: 20,
  introY: 44,
  selectsY: 116, // labels sit at selectsY − 20
  menuY: 150,
  planY: 164,
  planCapY: 188,
  footerY: 262,
  colW: 178,
  col2X: 222,
} as const

export const ISSUE_DIALOG_RECT = {
  x: Math.round((WIN.w - I.w) / 2),
  y: Math.round((WIN.h - I.h) / 2),
  w: I.w,
  h: I.h,
} as const

export const IssueCodingDialog: React.FC<{
  frame: number
  appearAt: number
  modelMenu?: SelectMenuSpec
  effortMenu?: SelectMenuSpec
  planCheckAt?: number
  buttonState?: DialogButtonState
  collapseAt?: number
  x?: number
  y?: number
}> = ({ frame, appearAt, modelMenu, effortMenu, planCheckAt, buttonState, collapseAt, x = ISSUE_DIALOG_RECT.x, y = ISSUE_DIALOG_RECT.y }) => {
  const modelValue =
    modelMenu?.highlight !== undefined && frame >= modelMenu.closeAt ? modelMenu.highlight.option : "Fable"
  const effortValue =
    effortMenu?.highlight !== undefined && frame >= effortMenu.closeAt ? effortMenu.highlight.option : "CLI default"
  return (
    <DialogShell frame={frame} appearAt={appearAt} collapseAt={collapseAt} x={x} y={y} w={I.w} h={I.h} title={`Start coding on ${HERO.id}`}>
      <div style={captionStyle(I.pad, I.introY, I.w - 2 * I.pad)}>
        Claude works on {HERO.id} in its own worktree and opens the pull request when done.
      </div>

      <SelectBox frame={frame} x={I.pad} boxY={I.selectsY} w={I.colW} label="Model" value={modelValue} />
      <SelectBox frame={frame} x={I.col2X} boxY={I.selectsY} w={I.colW} label="Effort" value={effortValue} />

      {/* Plan mode row + caption */}
      <CheckBox frame={frame} x={I.pad} y={I.planY} checked={false} checkAt={planCheckAt} />
      <div
        style={{
          position: "absolute",
          left: I.pad + 24,
          top: I.planY - 1,
          fontSize: 13.5,
          fontWeight: 600,
          color: C.text,
          lineHeight: "18px",
        }}
      >
        Plan mode
      </div>
      <div style={captionStyle(I.pad + 24, I.planCapY, I.w - I.pad - (I.pad + 24))}>{PLAN_CAPTION}</div>

      <FooterButtons frame={frame} y={I.footerY} rightEdge={I.w - I.pad} state={buttonState} />

      {/* Popover menus overlay everything below the selects */}
      {modelMenu ? <SelectMenu frame={frame} x={I.pad} y={I.menuY} w={I.colW} spec={modelMenu} initial="Fable" /> : null}
      {effortMenu ? <SelectMenu frame={frame} x={I.col2X} y={I.menuY} w={I.colW} spec={effortMenu} initial="CLI default" /> : null}
    </DialogShell>
  )
}

// Window-local anchor points for the cursor (option i = default option lists).
export const ISSUE_DIALOG_ANCHORS = (() => {
  const { x, y } = ISSUE_DIALOG_RECT
  const optionY = (i: number) => y + I.menuY + MENU_PAD + MENU_ROW_H / 2 + MENU_ROW_H * i
  return {
    close: { x: x + I.w - 24, y: y + 24 },
    modelSelect: { x: x + I.pad + I.colW / 2, y: y + I.selectsY + 15 },
    effortSelect: { x: x + I.col2X + I.colW / 2, y: y + I.selectsY + 15 },
    modelOptions: MODEL_OPTIONS.map((option, i) => ({ option, x: x + I.pad + I.colW / 2, y: optionY(i) })),
    effortOptions: EFFORT_OPTIONS.map((option, i) => ({ option, x: x + I.col2X + I.colW / 2, y: optionY(i) })),
    planCheckbox: { x: x + I.pad + 8, y: y + I.planY + 8 },
    cancel: { x: x + I.w - I.pad - PRIMARY_W - 8 - CANCEL_W / 2, y: y + I.footerY + BTN_H / 2 },
    startCoding: { x: x + I.w - I.pad - PRIMARY_W / 2, y: y + I.footerY + BTN_H / 2 },
  } as const
})()

// ── Release variant (560px, S9) ───────────────────────────────────────────────
const R = {
  w: 560,
  h: 582,
  pad: 20,
  introY: 44,
  repoY: 88,
  rowsY: 108,
  rowH: 26,
  selects1Y: 272, // Model / Effort (labels at −20)
  capY: 308, // "ultracode sets effort"
  selects2Y: 354, // Subagent model / effort
  dwY: 400, // Dynamic workflows row
  dwCapY: 424,
  planY: 454,
  planCapY: 478,
  footerY: 534,
  colW: 244,
  col2X: 296,
} as const

export const RELEASE_DIALOG_RECT = {
  x: Math.round((WIN.w - R.w) / 2),
  y: Math.round((WIN.h - R.h) / 2),
  w: R.w,
  h: R.h,
} as const

// Ref shows the repo group header capitalized ("Niach/exponential").
const REPO_HEADER = IDENTITY.repo.charAt(0).toUpperCase() + IDENTITY.repo.slice(1)

const STATUS_LABEL: Record<IssueStatus, string> = {
  backlog: "Backlog",
  todo: "Todo",
  in_progress: "In Progress",
  done: "Done",
}
// Trailing muted right-label per checklist row (ref shows "PR merged" there —
// ours are OPEN issues about to run, so default to their board status).
const defaultRightLabel = (id: string): string => {
  const row = BOARD.find((r) => r.id === id)
  return row ? STATUS_LABEL[row.status] : "Todo"
}

// Light track + dark thumb (ref pixel truth), thumb pulse at `pulseAt`.
const SwitchOn: React.FC<{ frame: number; x: number; y: number; pulseAt?: number }> = ({ frame, x, y, pulseAt }) => (
  <div
    style={{
      position: "absolute",
      left: x,
      top: y,
      width: 38,
      height: 22,
      borderRadius: 999,
      backgroundColor: C.primary,
    }}
  >
    <div
      style={{
        position: "absolute",
        left: 18,
        top: 2,
        width: 18,
        height: 18,
        borderRadius: "50%",
        backgroundColor: C.bg,
        scale: String(1 + 0.15 * pulse(frame, pulseAt, 4, 8)),
      }}
    />
  </div>
)

export const ReleaseCodingDialog: React.FC<{
  frame: number
  appearAt: number
  checkShimmerAt?: number // checked rows shimmer-cascade (2f stagger)
  ultraPulseAt?: number // ultracode thumb pulse + caption brighten
  selectsFlickAt?: number // value-flick cascade over the 4 selects (6f stagger)
  buttonState?: DialogButtonState
  collapseAt?: number
  invalid?: boolean // shows "Select at least one issue." + dims the primary
  rowRightLabels?: Record<string, string>
  x?: number
  y?: number
}> = ({
  frame,
  appearAt,
  checkShimmerAt,
  ultraPulseAt,
  selectsFlickAt,
  buttonState,
  collapseAt,
  invalid = false,
  rowRightLabels,
  x = RELEASE_DIALOG_RECT.x,
  y = RELEASE_DIALOG_RECT.y,
}) => {
  const starting = buttonState?.startingAt
  const capO =
    ultraPulseAt === undefined
      ? 1
      : interpolate(frame, [ultraPulseAt, ultraPulseAt + 12], [0.6, 1], EASED)
  return (
    <DialogShell frame={frame} appearAt={appearAt} collapseAt={collapseAt} x={x} y={y} w={R.w} h={R.h} title="Start coding on release">
      <div style={captionStyle(R.pad, R.introY, R.w - 2 * R.pad)}>
        One Claude orchestrator implements the checked issues of “{RELEASE.name}” — one subagent per issue.
      </div>

      {/* Repo group header + checklist */}
      <div
        style={{
          position: "absolute",
          left: R.pad,
          top: R.repoY,
          fontSize: 12,
          fontWeight: 600,
          color: C.muted,
          lineHeight: "16px",
        }}
      >
        {REPO_HEADER}
      </div>
      {RELEASE.dialogIssues.map((issue, i) => {
        const rowTop = R.rowsY + R.rowH * i
        const shimmer = checkShimmerAt === undefined ? undefined : checkShimmerAt + i * 2
        const band = pulse(frame, shimmer, 3, 12)
        return (
          <React.Fragment key={issue.id}>
            {band > 0 ? (
              <div
                style={{
                  position: "absolute",
                  left: R.pad - 6,
                  top: rowTop - 2,
                  width: R.w - 2 * R.pad + 12,
                  height: R.rowH + 2,
                  borderRadius: 4,
                  backgroundColor: `rgba(255,255,255,${0.07 * band})`,
                }}
              />
            ) : null}
            <CheckBox frame={frame} x={R.pad} y={rowTop + 3} checked pulseAt={shimmer} />
            <div
              style={{
                position: "absolute",
                left: R.pad + 26,
                top: rowTop + 5,
                width: 56,
                fontFamily: MONO_FONT,
                fontSize: 11,
                color: C.muted,
                lineHeight: "16px",
              }}
            >
              {issue.id}
            </div>
            <div
              style={{
                position: "absolute",
                left: R.pad + 88,
                top: rowTop + 3,
                width: R.w - 2 * R.pad - 88 - 84,
                fontSize: 13,
                color: C.text,
                lineHeight: "20px",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {issue.title}
            </div>
            <div
              style={{
                position: "absolute",
                right: R.pad,
                top: rowTop + 5,
                fontSize: 12,
                color: C.muted,
                lineHeight: "16px",
              }}
            >
              {(rowRightLabels ?? {})[issue.id] ?? defaultRightLabel(issue.id)}
            </div>
          </React.Fragment>
        )
      })}

      {/* Model / Effort (effort disabled — ultracode sets it) */}
      <SelectBox frame={frame} x={R.pad} boxY={R.selects1Y} w={R.colW} label="Model" value="Fable" flickAt={selectsFlickAt} />
      <SelectBox
        frame={frame}
        x={R.col2X}
        boxY={R.selects1Y}
        w={R.colW}
        label="Effort"
        value="CLI default"
        disabled
        flickAt={selectsFlickAt === undefined ? undefined : selectsFlickAt + 6}
      />
      <div style={captionStyle(R.col2X, R.capY, R.colW)}>ultracode sets effort</div>

      {/* Subagent model / effort */}
      <SelectBox
        frame={frame}
        x={R.pad}
        boxY={R.selects2Y}
        w={R.colW}
        label="Subagent model"
        value="Inherit"
        flickAt={selectsFlickAt === undefined ? undefined : selectsFlickAt + 12}
      />
      <SelectBox
        frame={frame}
        x={R.col2X}
        boxY={R.selects2Y}
        w={R.colW}
        label="Subagent effort"
        value="Inherit"
        flickAt={selectsFlickAt === undefined ? undefined : selectsFlickAt + 18}
      />

      {/* Dynamic workflows (ultracode) switch row */}
      <div
        style={{
          position: "absolute",
          left: R.pad,
          top: R.dwY,
          fontSize: 13.5,
          fontWeight: 600,
          color: C.text,
          lineHeight: "20px",
        }}
      >
        Dynamic workflows (ultracode)
      </div>
      <SwitchOn frame={frame} x={R.w - R.pad - 38} y={R.dwY - 1} pulseAt={ultraPulseAt} />
      <div style={{ ...captionStyle(R.pad, R.dwCapY, R.w - 2 * R.pad), opacity: capO }}>
        Runs the orchestrator with --effort ultracode — works with any model.
      </div>

      {/* Plan mode row + caption */}
      <CheckBox frame={frame} x={R.pad} y={R.planY} checked={false} />
      <div
        style={{
          position: "absolute",
          left: R.pad + 24,
          top: R.planY - 1,
          fontSize: 13.5,
          fontWeight: 600,
          color: C.text,
          lineHeight: "18px",
        }}
      >
        Plan mode
      </div>
      <div style={captionStyle(R.pad + 24, R.planCapY, R.w - R.pad - (R.pad + 24))}>{PLAN_CAPTION}</div>

      {/* Footer */}
      {invalid ? (
        <div
          style={{
            position: "absolute",
            left: R.pad,
            top: R.footerY + 6,
            fontSize: 12,
            color: C.muted,
            lineHeight: "16px",
          }}
        >
          Select at least one issue.
        </div>
      ) : null}
      <FooterButtons frame={frame} y={R.footerY} rightEdge={R.w - R.pad} state={buttonState} dim={invalid} />

      {/* Indigo launch wash traveling down on Start (storyboard S9 f1068) */}
      {starting !== undefined && frame >= starting && frame <= starting + 14 ? (
        <div style={{ position: "absolute", inset: 0, borderRadius: 8, overflow: "hidden", pointerEvents: "none" }}>
          <div
            style={{
              position: "absolute",
              left: 0,
              right: 0,
              top: 0,
              height: 140,
              translate: `0px ${interpolate(frame, [starting, starting + 12], [-140, R.h], EASED)}px`,
              opacity: interpolate(frame, [starting, starting + 2, starting + 12], [0, 1, 0], CLAMP),
              // C.indigo #4f46e5 @ 22% wash
              background: "linear-gradient(to bottom, transparent, rgba(79,70,229,0.22), transparent)",
            }}
          />
        </div>
      ) : null}
    </DialogShell>
  )
}

// Window-local anchor points for the cursor.
export const RELEASE_DIALOG_ANCHORS = (() => {
  const { x, y } = RELEASE_DIALOG_RECT
  return {
    close: { x: x + R.w - 24, y: y + 24 },
    rows: RELEASE.dialogIssues.map((issue, i) => ({
      id: issue.id,
      checkbox: { x: x + R.pad + 8, y: y + R.rowsY + R.rowH * i + 11 },
      row: { x: x + R.w / 2, y: y + R.rowsY + R.rowH * i + 13 },
    })),
    modelSelect: { x: x + R.pad + R.colW / 2, y: y + R.selects1Y + 15 },
    effortSelect: { x: x + R.col2X + R.colW / 2, y: y + R.selects1Y + 15 },
    subagentModelSelect: { x: x + R.pad + R.colW / 2, y: y + R.selects2Y + 15 },
    subagentEffortSelect: { x: x + R.col2X + R.colW / 2, y: y + R.selects2Y + 15 },
    ultracodeToggle: { x: x + R.w - R.pad - 19, y: y + R.dwY + 10 },
    planCheckbox: { x: x + R.pad + 8, y: y + R.planY + 8 },
    cancel: { x: x + R.w - R.pad - PRIMARY_W - 8 - CANCEL_W / 2, y: y + R.footerY + BTN_H / 2 },
    startCoding: { x: x + R.w - R.pad - PRIMARY_W / 2, y: y + R.footerY + BTN_H / 2 },
  } as const
})()
