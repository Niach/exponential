// closedloop/surfaces/startdialog.tsx — the ONE unified Start-coding dialog
// (apps/desktop/crates/ui/src/start_coding_dialog.rs, EXP-213/EXP-206 state):
// muted intro line, "Search issues…" field, BOXED scrollable checklist
// (EXP-151 checked, open siblings unchecked — telegraphs multi-select), the
// agent tab strip (Claude Code active · Codex · pi), per-agent Model/Effort
// selects, and the hint-free toggle stack — "Dynamic workflows (ultracode)"
// (OFF), "Plan mode" (CHECKED — Claude default) and "Skip permissions" (OFF).
// Built from the ships dialog primitives so it renders as a sibling of the
// ships dialogs.

import React from "react"
import { interpolate } from "remotion"
import { C, MONO_FONT, WIN } from "../../ships/theme"
import {
  BTN_H,
  CANCEL_W,
  CheckBox,
  DialogShell,
  FooterButtons,
  PRIMARY_W,
  SelectBox,
  captionStyle,
  type DialogButtonState,
} from "../../ships/surfaces/dialogs"
import { DIALOG_ISSUES } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// 0→1→0 pulse starting at `at`.
const pulse = (frame: number, at: number | undefined, up = 3, down = 9) =>
  at === undefined ? 0 : interpolate(frame, [at, at + up, at + down], [0, 1, 0], CLAMP)

// ── Geometry (dialog-local) ───────────────────────────────────────────────────
const D = {
  w: 560,
  h: 500,
  pad: 20,
  introY: 40, // two muted lines
  searchY: 84, // h 30
  boxY: 122, // bordered checklist box (EXP-213: boxed like the web picker)
  boxPad: 8,
  rowH: 26, // 4 rows → box h = 4·26 + 2·8 = 120
  tabsY: 252, // agent tab strip, h 28
  selectsY: 320, // Model/Effort boxes (labels at −20)
  ucY: 366,
  planY: 392,
  skipY: 418,
  footerY: 452,
  colW: 244,
  col2X: 296,
} as const
const ROWS_Y = D.boxY + D.boxPad
const BOX_H = DIALOG_ISSUES.length * D.rowH + 2 * D.boxPad

export const START_DIALOG_RECT = {
  x: Math.round((WIN.w - D.w) / 2),
  y: Math.round((WIN.h - D.h) / 2),
  w: D.w,
  h: D.h,
} as const

const SearchGlyph: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </svg>
)

// ── Agent brand marks (desktop assets/icons/{claude,codex,pi}.svg twins) ─────
const ClaudeMark: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 100 100" fill="currentColor" style={{ display: "block" }}>
    <path d="m19.6 66.5 19.7-11 .3-1-.3-.5h-1l-3.3-.2-11.2-.3L14 53l-9.5-.5-2.4-.5L0 49l.2-1.5 2-1.3 2.9.2 6.3.5 9.5.6 6.9.4L38 49.1h1.6l.2-.7-.5-.4-.4-.4L29 41l-10.6-7-5.6-4.1-3-2-1.5-2-.6-4.2 2.7-3 3.7.3.9.2 3.7 2.9 8 6.1L37 36l1.5 1.2.6-.4.1-.3-.7-1.1L33 25l-6-10.4-2.7-4.3-.7-2.6c-.3-1-.4-2-.4-3l3-4.2L28 0l4.2.6L33.8 2l2.6 6 4.1 9.3L47 29.9l2 3.8 1 3.4.3 1h.7v-.5l.5-7.2 1-8.7 1-11.2.3-3.2 1.6-3.8 3-2L61 2.6l2 2.9-.3 1.8-1.1 7.7L59 27.1l-1.5 8.2h.9l1-1.1 4.1-5.4 6.9-8.6 3-3.5L77 13l2.3-1.8h4.3l3.1 4.7-1.4 4.9-4.4 5.6-3.7 4.7-5.3 7.1-3.2 5.7.3.4h.7l12-2.6 6.4-1.1 7.6-1.3 3.5 1.6.4 1.6-1.4 3.4-8.2 2-9.6 2-14.3 3.3-.2.1.2.3 6.4.6 2.8.2h6.8l12.6 1 3.3 2 1.9 2.7-.3 2-5.1 2.6-6.8-1.6-16-3.8-5.4-1.3h-.8v.4l4.6 4.5 8.3 7.5L89 80.1l.5 2.4-1.3 2-1.4-.2-9.2-7-3.6-3-8-6.8h-.5v.7l1.8 2.7 9.8 14.7.5 4.5-.7 1.4-2.6 1-2.7-.6-5.8-8-6-9-4.7-8.2-.5.4-2.9 30.2-1.3 1.5-3 1.2-2.5-2-1.4-3 1.4-6.2 1.6-8 1.3-6.4 1.2-7.9.7-2.6v-.2H49L43 72l-9 12.3-7.2 7.6-1.7.7-3-1.5.3-2.8L24 86l10-12.8 6-7.9 4-4.6-.1-.5h-.3L17.2 77.4l-4.7.6-2-2 .2-3 1-1 8-5.5Z" />
  </svg>
)

const CodexMark: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" style={{ display: "block" }}>
    <path d="M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z" />
  </svg>
)

const PiMark: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 800 800" fill="currentColor" style={{ display: "block" }}>
    <path d="M165.29,165.29 H517.36 V282.65 H165.29 Z M165.29,282.65 H282.65 V634.72 H165.29 Z M400,282.65 H517.36 V400 H400 Z M282.65,400 H400 V517.36 H282.65 Z M517.36,400 H634.72 V634.72 H517.36 Z" />
  </svg>
)

// The agent tab strip (EXP-201/EXP-206): centered pill tabs, brand mark +
// name, Claude Code active.
const AGENT_TABS = [
  { id: "claude", label: "Claude Code", Mark: ClaudeMark, active: true },
  { id: "codex", label: "Codex", Mark: CodexMark, active: false },
  { id: "pi", label: "pi", Mark: PiMark, active: false },
] as const

const AgentTabStrip: React.FC = () => (
  <div
    style={{
      position: "absolute",
      left: D.pad,
      top: D.tabsY,
      width: D.w - 2 * D.pad,
      display: "flex",
      justifyContent: "center",
      gap: 6,
    }}
  >
    {AGENT_TABS.map(({ id, label, Mark, active }) => (
      <div
        key={id}
        style={{
          height: 28,
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          gap: 7,
          padding: "0 12px",
          borderRadius: 999,
          border: `1px solid ${active ? C.border : "transparent"}`,
          backgroundColor: active ? C.accentBg : "transparent",
          color: active ? C.text : C.muted,
        }}
      >
        <Mark size={12} />
        <span style={{ fontSize: 12.5, fontWeight: 500, lineHeight: "16px" }}>{label}</span>
      </div>
    ))}
  </div>
)

// A hint-free checkbox toggle row (EXP-206 — gpui Checkbox().label()).
const ToggleRow: React.FC<{ frame: number; y: number; label: string; checked: boolean }> = ({ frame, y, label, checked }) => (
  <>
    <CheckBox frame={frame} x={D.pad} y={y} checked={checked} />
    <div
      style={{
        position: "absolute",
        left: D.pad + 24,
        top: y - 1,
        fontSize: 13.5,
        fontWeight: 600,
        color: C.text,
        lineHeight: "18px",
      }}
    >
      {label}
    </div>
  </>
)

export type StartCodingDialogProps = {
  frame: number
  appearAt: number
  checkPulseAt?: number // shimmer on the pre-checked EXP-151 row
  rowHover?: { index: number; at: number; out: number } // hover band over an unchecked row (multi-select telegraph)
  buttonState?: DialogButtonState
  collapseAt?: number
  x?: number
  y?: number
}

export const StartCodingDialog: React.FC<StartCodingDialogProps> = ({
  frame,
  appearAt,
  checkPulseAt,
  rowHover,
  buttonState,
  collapseAt,
  x = START_DIALOG_RECT.x,
  y = START_DIALOG_RECT.y,
}) => {
  return (
    <DialogShell frame={frame} appearAt={appearAt} collapseAt={collapseAt} x={x} y={y} w={D.w} h={D.h} title="Start coding">
      {/* intro line (checked_count == 1 wording) */}
      <div style={captionStyle(D.pad, D.introY, D.w - 2 * D.pad)}>
        Claude Code works on the checked issue in its own worktree and opens the pull request when done. Check more issues for a batch run.
      </div>

      {/* search field */}
      <div
        style={{
          position: "absolute",
          left: D.pad,
          top: D.searchY,
          width: D.w - 2 * D.pad,
          height: 30,
          boxSizing: "border-box",
          borderRadius: 6,
          border: `1px solid ${C.input}`,
          backgroundColor: "rgba(255,255,255,0.04)",
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 10px",
          color: C.muted,
        }}
      >
        <SearchGlyph size={13} />
        <span style={{ fontSize: 13 }}>Search issues…</span>
      </div>

      {/* boxed checklist (EXP-213: boxed like the web picker) */}
      <div
        style={{
          position: "absolute",
          left: D.pad,
          top: D.boxY,
          width: D.w - 2 * D.pad,
          height: BOX_H,
          boxSizing: "border-box",
          borderRadius: 6,
          border: `1px solid ${C.border}`,
        }}
      />
      {DIALOG_ISSUES.map((issue, i) => {
        const rowTop = ROWS_Y + D.rowH * i
        const shimmer = issue.checked === true ? checkPulseAt : undefined
        const band = pulse(frame, shimmer, 3, 12)
        const hoverBand =
          rowHover !== undefined && rowHover.index === i
            ? interpolate(frame, [rowHover.at, rowHover.at + 4], [0, 1], CLAMP) *
              interpolate(frame, [rowHover.out, rowHover.out + 4], [1, 0], CLAMP)
            : 0
        const tint = Math.max(0.07 * band, 0.05 * hoverBand)
        return (
          <React.Fragment key={issue.id}>
            {tint > 0 ? (
              <div
                style={{
                  position: "absolute",
                  left: D.pad + 4,
                  top: rowTop - 1,
                  width: D.w - 2 * D.pad - 8,
                  height: D.rowH,
                  borderRadius: 4,
                  backgroundColor: `rgba(255,255,255,${tint})`,
                }}
              />
            ) : null}
            <CheckBox frame={frame} x={D.pad + 10} y={rowTop + 3} checked={issue.checked === true} pulseAt={shimmer} />
            <div
              style={{
                position: "absolute",
                left: D.pad + 36,
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
                left: D.pad + 98,
                top: rowTop + 3,
                width: D.w - 2 * D.pad - 98 - 92,
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
                right: D.pad + 10,
                top: rowTop + 5,
                fontSize: 12,
                color: C.muted,
                lineHeight: "16px",
              }}
            >
              {issue.right}
            </div>
          </React.Fragment>
        )
      })}

      {/* agent tab strip */}
      <AgentTabStrip />

      {/* Model / Effort (per-agent lists — Claude defaults) */}
      <SelectBox frame={frame} x={D.pad} boxY={D.selectsY} w={D.colW} label="Model" value="Fable" />
      <SelectBox frame={frame} x={D.col2X} boxY={D.selectsY} w={D.colW} label="Effort" value="CLI default" />

      {/* toggle stack (hint-free, EXP-206; Claude defaults: plan ON only) */}
      <ToggleRow frame={frame} y={D.ucY} label="Dynamic workflows (ultracode)" checked={false} />
      <ToggleRow frame={frame} y={D.planY} label="Plan mode" checked />
      <ToggleRow frame={frame} y={D.skipY} label="Skip permissions" checked={false} />

      <FooterButtons frame={frame} y={D.footerY} rightEdge={D.w - D.pad} state={buttonState} />
    </DialogShell>
  )
}

// Window-local cursor anchors.
export const START_DIALOG_ANCHORS = (() => {
  const { x, y } = START_DIALOG_RECT
  return {
    close: { x: x + D.w - 24, y: y + 24 },
    search: { x: x + D.w / 2, y: y + D.searchY + 15 },
    rows: DIALOG_ISSUES.map((issue, i) => ({
      id: issue.id,
      row: { x: x + D.w / 2, y: y + ROWS_Y + D.rowH * i + 13 },
      checkbox: { x: x + D.pad + 18, y: y + ROWS_Y + D.rowH * i + 11 },
    })),
    modelSelect: { x: x + D.pad + D.colW / 2, y: y + D.selectsY + 15 },
    effortSelect: { x: x + D.col2X + D.colW / 2, y: y + D.selectsY + 15 },
    ultracodeToggle: { x: x + D.pad + 8, y: y + D.ucY + 8 },
    planCheckbox: { x: x + D.pad + 8, y: y + D.planY + 8 },
    skipCheckbox: { x: x + D.pad + 8, y: y + D.skipY + 8 },
    cancel: { x: x + D.w - D.pad - PRIMARY_W - 8 - CANCEL_W / 2, y: y + D.footerY + BTN_H / 2 },
    startCoding: { x: x + D.w - D.pad - PRIMARY_W / 2, y: y + D.footerY + BTN_H / 2 },
  } as const
})()
