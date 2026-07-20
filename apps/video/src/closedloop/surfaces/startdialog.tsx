// closedloop/surfaces/startdialog.tsx — the ONE unified Start-coding dialog
// (apps/desktop/crates/ui/src/start_coding_dialog.rs): always a searchable
// multi-issue picker — "Search issues…" field, checkbox list (EXP-151 checked,
// open siblings unchecked — telegraphs multi-select), Model/Effort selects,
// the "Dynamic workflows (ultracode)" switch (OFF — issue-run default) and the
// native "Plan mode" checkbox (CHECKED — issue-run default). Built from the
// ships dialog primitives so it renders as a sibling of the ships dialogs.

import React from "react"
import { interpolate } from "remotion"
import { C, MONO_FONT, WIN } from "../../ships/theme"
import {
  BTN_H,
  CANCEL_W,
  CheckBox,
  DialogShell,
  FooterButtons,
  PLAN_CAPTION,
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
  h: 468,
  pad: 20,
  searchY: 44,
  rowsY: 90,
  rowH: 28,
  selectsY: 246, // labels at −20
  dwY: 294,
  dwCapY: 318,
  planY: 350,
  planCapY: 374,
  footerY: 420,
  colW: 244,
  col2X: 296,
} as const

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

// Switch in the OFF state (issue-run default: ultracode off) — muted track,
// light thumb parked left.
const SwitchOff: React.FC<{ x: number; y: number }> = ({ x, y }) => (
  <div
    style={{
      position: "absolute",
      left: x,
      top: y,
      width: 38,
      height: 22,
      borderRadius: 999,
      backgroundColor: "rgba(255,255,255,0.15)",
    }}
  >
    <div style={{ position: "absolute", left: 2, top: 2, width: 18, height: 18, borderRadius: "50%", backgroundColor: C.muted }} />
  </div>
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
      {/* search field */}
      <div
        style={{
          position: "absolute",
          left: D.pad,
          top: D.searchY,
          width: D.w - 2 * D.pad,
          height: 32,
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

      {/* checklist */}
      {DIALOG_ISSUES.map((issue, i) => {
        const rowTop = D.rowsY + D.rowH * i
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
                  left: D.pad - 6,
                  top: rowTop - 2,
                  width: D.w - 2 * D.pad + 12,
                  height: D.rowH + 2,
                  borderRadius: 4,
                  backgroundColor: `rgba(255,255,255,${tint})`,
                }}
              />
            ) : null}
            <CheckBox frame={frame} x={D.pad} y={rowTop + 4} checked={issue.checked === true} pulseAt={shimmer} />
            <div
              style={{
                position: "absolute",
                left: D.pad + 26,
                top: rowTop + 6,
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
                left: D.pad + 88,
                top: rowTop + 4,
                width: D.w - 2 * D.pad - 88 - 92,
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
                right: D.pad,
                top: rowTop + 6,
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

      {/* Model / Effort */}
      <SelectBox frame={frame} x={D.pad} boxY={D.selectsY} w={D.colW} label="Model" value="Fable" />
      <SelectBox frame={frame} x={D.col2X} boxY={D.selectsY} w={D.colW} label="Effort" value="CLI default" />

      {/* Dynamic workflows (ultracode) — OFF for issue runs */}
      <div
        style={{
          position: "absolute",
          left: D.pad,
          top: D.dwY,
          fontSize: 13.5,
          fontWeight: 600,
          color: C.text,
          lineHeight: "20px",
        }}
      >
        Dynamic workflows (ultracode)
      </div>
      <SwitchOff x={D.w - D.pad - 38} y={D.dwY - 1} />
      <div style={captionStyle(D.pad, D.dwCapY, D.w - 2 * D.pad)}>
        Runs the agent with --effort ultracode — works with any model.
      </div>

      {/* Plan mode — CHECKED for issue runs */}
      <CheckBox frame={frame} x={D.pad} y={D.planY} checked />
      <div
        style={{
          position: "absolute",
          left: D.pad + 24,
          top: D.planY - 1,
          fontSize: 13.5,
          fontWeight: 600,
          color: C.text,
          lineHeight: "18px",
        }}
      >
        Plan mode
      </div>
      <div style={captionStyle(D.pad + 24, D.planCapY, D.w - D.pad - (D.pad + 24))}>{PLAN_CAPTION}</div>

      <FooterButtons frame={frame} y={D.footerY} rightEdge={D.w - D.pad} state={buttonState} />
    </DialogShell>
  )
}

// Window-local cursor anchors.
export const START_DIALOG_ANCHORS = (() => {
  const { x, y } = START_DIALOG_RECT
  return {
    close: { x: x + D.w - 24, y: y + 24 },
    search: { x: x + D.w / 2, y: y + D.searchY + 16 },
    rows: DIALOG_ISSUES.map((issue, i) => ({
      id: issue.id,
      row: { x: x + D.w / 2, y: y + D.rowsY + D.rowH * i + 14 },
      checkbox: { x: x + D.pad + 8, y: y + D.rowsY + D.rowH * i + 12 },
    })),
    modelSelect: { x: x + D.pad + D.colW / 2, y: y + D.selectsY + 15 },
    effortSelect: { x: x + D.col2X + D.colW / 2, y: y + D.selectsY + 15 },
    ultracodeToggle: { x: x + D.w - D.pad - 19, y: y + D.dwY + 10 },
    planCheckbox: { x: x + D.pad + 8, y: y + D.planY + 8 },
    cancel: { x: x + D.w - D.pad - PRIMARY_W - 8 - CANCEL_W / 2, y: y + D.footerY + BTN_H / 2 },
    startCoding: { x: x + D.w - D.pad - PRIMARY_W / 2, y: y + D.footerY + BTN_H / 2 },
  } as const
})()
