// closedloop/surfaces/startphone.tsx — the remote-start phone (EXP-388,
// matched to the shipping mobile UI): the REAL issue detail screen for
// EXP-151 underneath, and the REAL StartCodingSheet over it — a full-height
// GlassSheetChrome with NO bar buttons (EXP-687: the confirm is the ONE
// pinned button and a swipe down cancels, so the sheet opens on its grabber),
// the Issues|Actions|Chat glass segmented control, the "Issues" section with its inline search row and the checkbox
// issue rows (EXP-151 checked and pinned first), the agent capsule strip
// (Claude Code · Codex · pi), the Model + Effort picker rows and the
// launch toggles, and the pinned full-width `GlassSubmitButton` ("Start
// coding", primary fill #e5e5e5 on primaryForeground text). One desktop
// online = no Device row (like the app); after the start the "Start sent to
// MacBook Pro" capsule toast confirms.
//
// EVERY number in the sheet is authored in iOS POINTS on the 414pt canvas and
// scaled ONCE through `pt()` — the same measurements the marketing page's
// sibling recreation carries (src/mobile/StartCodingSheet.tsx + the `mss-*`
// rules in styles/mobile.css, traced off shots/start-coding/ios.webp), so the
// film and the page never drift apart into hand-tuned marketing px.
// All frame props are COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, SETTLE, UI_FONT } from "../../ships/theme"
import { ClaudeMark, CodexMark, PiMark } from "./agentmarks"
import { CL_BOARD, CL_ISSUE, CL_LABELS, PHONE_START, REPORT } from "../fixtures"
import { PHONE, PhoneChassis } from "./steerphone"
import {
  Glyph,
  IssueScreen,
  MPriorityIcon,
  MStatusIcon,
  type MobilePriority,
  type MobileStatus,
} from "./mobileui"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

// iOS points → phone px. The chassis screen is 312px wide for the 414pt
// device, so ONE factor carries every measured value across.
const PT = PHONE.screenW / 414
const pt = (v: number): number => Math.round(v * PT * 10) / 10

// The sheet is a near-full-height iOS sheet: it stops just under the status
// row and covers the issue detail behind it, exactly like the real one.
const SHEET_TOP = pt(59)
const SHEET_H = PHONE.screenH - SHEET_TOP

// Glass tokens the sheet's own controls speak (mobile GlassTheme).
const G = {
  pillBg: "rgba(255,255,255,0.07)",
  pillStroke: "rgba(255,255,255,0.10)",
  segBg: "rgba(255,255,255,0.10)",
  segStroke: "rgba(255,255,255,0.10)",
  segActive: "rgba(255,255,255,0.12)",
  card: "rgba(255,255,255,0.05)",
  sep: "rgba(255,255,255,0.10)",
  header: "rgba(255,255,255,0.66)",
  placeholder: "rgba(255,255,255,0.35)",
  value: "rgba(255,255,255,0.82)",
  chev: "rgba(255,255,255,0.50)",
  id: "rgba(255,255,255,0.62)",
  rowChecked: "rgba(255,255,255,0.07)",
  segLabel: "rgba(255,255,255,0.85)",
  track: "#64636a",
} as const

// DesignTokens.Palette.primary / primaryForeground — the submit button's fill
// and text (generated in ExpUI's DesignTokens.generated.swift).
const PRIMARY_FILL = "#e5e5e5"
const PRIMARY_FG = "#171717"


const Spinner: React.FC<{ frame: number; size?: number }> = ({
  frame,
  size = pt(15),
}) => (
  <span style={{ display: "flex", rotate: `${(frame * 24) % 360}deg` }}>
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.4}
      strokeLinecap="round"
    >
      <path d="M21 12a9 9 0 1 1-6.2-8.56" />
    </svg>
  </span>
)

// ── The issue pool the picker offers ────────────────────────────────────────
// The sheet's own filter: repo-backed, non-terminal issues, with the checked
// one pinned FIRST (the pin order is snapshotted at open, EXP-241).
type SheetIssue = {
  id: string
  title: string
  status: MobileStatus
  priority: MobilePriority
}
const CHECKED_ID = CL_ISSUE.id
const SHEET_ISSUES: SheetIssue[] = [
  ...CL_BOARD.filter((row) => row.id === CHECKED_ID),
  ...CL_BOARD.filter((row) => row.id !== CHECKED_ID && row.status !== "done"),
].map(({ id, title, status, priority }) => ({ id, title, status, priority }))

// ── The glass segmented control (both strips are the SAME control) ───────────
const SEG_H = pt(38.5)
const SEG_INSET = pt(18.5)

const SegControl: React.FC<{
  top: number
  items: readonly {
    id: string
    label: string
    Mark?: React.FC<{ size: number }>
  }[]
  activeId: string
  flick?: { id: string; t: number }
}> = ({ top, items, activeId, flick }) => (
  <div
    style={{
      position: "absolute",
      left: SEG_INSET,
      right: SEG_INSET,
      top,
      height: SEG_H,
      boxSizing: "border-box",
      display: "flex",
      gap: pt(4),
      padding: pt(4),
      borderRadius: 999,
      backgroundColor: G.segBg,
      border: `1px solid ${G.segStroke}`,
    }}
  >
    {items.map(({ id, label, Mark }) => {
      const active = id === activeId
      const hot = !active && flick?.id === id ? flick.t : 0
      return (
        <span
          key={id}
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: pt(6),
            borderRadius: 999,
            backgroundColor: active
              ? G.segActive
              : hot > 0
                ? `rgba(255,255,255,${(0.06 * hot).toFixed(3)})`
                : "transparent",
            color: active ? C.text : G.segLabel,
            fontSize: pt(15),
            fontWeight: active ? 600 : 400,
            whiteSpace: "nowrap",
          }}
        >
          {Mark ? <Mark size={pt(13)} /> : null}
          {label}
        </span>
      )
    })}
  </div>
)

const TABS = [
  { id: "issues", label: "Issues" },
  { id: "actions", label: "Actions" },
  { id: "chat", label: "Chat" },
] as const

const AGENTS = [
  { id: "claude", label: "Claude Code", Mark: ClaudeMark },
  { id: "codex", label: "Codex", Mark: CodexMark },
  { id: "pi", label: "pi", Mark: PiMark },
] as const

// ── Grouped-card atoms ──────────────────────────────────────────────────────
const ROW_H = pt(49)
const CARD_INSET = pt(18.5)
const ROW_PAD = pt(19.5)

const Card: React.FC<{ top: number; children: React.ReactNode }> = ({
  top,
  children,
}) => (
  <div
    style={{
      position: "absolute",
      left: CARD_INSET,
      right: CARD_INSET,
      top,
      borderRadius: pt(22),
      backgroundColor: G.card,
      overflow: "hidden",
    }}
  >
    {children}
  </div>
)

const Sep: React.FC = () => (
  <div
    style={{ height: 1, margin: `0 ${ROW_PAD}px`, backgroundColor: G.sep }}
  />
)

// Label left, value + a TRAILING chevron right (a picker row, not a stepper).
const PickerRow: React.FC<{ label: string; value: string }> = ({
  label,
  value,
}) => (
  <div
    style={{
      height: ROW_H,
      boxSizing: "border-box",
      display: "flex",
      alignItems: "center",
      padding: `0 ${ROW_PAD}px`,
      fontSize: pt(17),
      color: C.text,
    }}
  >
    <span style={{ flex: 1 }}>{label}</span>
    <span style={{ color: G.value, whiteSpace: "nowrap" }}>{value}</span>
    <span style={{ marginLeft: pt(8.5), color: G.chev, display: "flex" }}>
      <Glyph size={pt(12)} sw={2}>
        <path d="m9 18 6-6-6-6" />
      </Glyph>
    </span>
  </div>
)

// iOS 26 switch, OFF: a wide white capsule knob parked left on a light track.
const ToggleRow: React.FC<{ label: string }> = ({ label }) => (
  <div
    style={{
      height: ROW_H,
      boxSizing: "border-box",
      display: "flex",
      alignItems: "center",
      paddingLeft: ROW_PAD,
      paddingRight: pt(17),
      fontSize: pt(17),
      color: C.text,
    }}
  >
    <span style={{ flex: 1 }}>{label}</span>
    <span
      style={{
        position: "relative",
        width: pt(60.5),
        height: pt(27),
        borderRadius: 999,
        backgroundColor: G.track,
        flexShrink: 0,
      }}
    >
      <span
        style={{
          position: "absolute",
          left: pt(3),
          top: pt(2.25),
          width: pt(34.5),
          height: pt(22.5),
          borderRadius: 999,
          backgroundColor: "#fff",
        }}
      />
    </span>
  </div>
)

// ── Issue picker row: radio · priority · identifier · status · title ─────────
const IssueRow: React.FC<{ issue: SheetIssue; checked: boolean }> = ({
  issue,
  checked,
}) => (
  <div
    style={{
      height: pt(28),
      boxSizing: "border-box",
      margin: `0 ${pt(18.5)}px`,
      display: "flex",
      alignItems: "center",
      borderRadius: pt(8),
      backgroundColor: checked ? G.rowChecked : "transparent",
      fontSize: pt(15),
    }}
  >
    <span
      style={{
        color: checked ? "#ffffff" : "rgba(255,255,255,0.6)",
        display: "flex",
        flexShrink: 0,
      }}
    >
      <Glyph size={pt(17)} sw={2}>
        <circle cx="12" cy="12" r="10" />
        {checked ? <path d="m9 12 2 2 4-4" /> : null}
      </Glyph>
    </span>
    <span style={{ marginLeft: pt(9), display: "flex" }}>
      <MPriorityIcon priority={issue.priority} size={pt(13)} />
    </span>
    <span
      style={{
        width: pt(60),
        marginLeft: pt(12),
        fontFamily: MONO_FONT,
        fontSize: pt(12),
        color: G.id,
        flexShrink: 0,
      }}
    >
      {issue.id}
    </span>
    <span style={{ marginLeft: pt(4.5), display: "flex" }}>
      <MStatusIcon status={issue.status} size={pt(15)} />
    </span>
    <span
      style={{
        marginLeft: pt(9),
        paddingRight: pt(2),
        color: C.text,
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
      }}
    >
      {issue.title}
    </span>
  </div>
)

// ── Sheet layout (sheet-local Ys, all derived — nothing hand-placed) ─────────
// EXP-687 killed the bar buttons, so the sheet starts on its grabber and the
// segmented control is the first content row (shots/start-coding/ios.webp).
const Y_GRABBER = pt(6)
const Y_TABS = pt(43)
const Y_HEADER = Y_TABS + SEG_H + pt(24.5)
const HEADER_H = pt(22)
const Y_PICKER = Y_HEADER + HEADER_H + pt(10)
const LIST_PAD = pt(12.5)
const PICKER_H = ROW_H + 1 + LIST_PAD * 2 + SHEET_ISSUES.length * pt(28)
const Y_AGENTS = Y_PICKER + PICKER_H + pt(16.5)
const Y_OPTIONS = Y_AGENTS + SEG_H + pt(16.5)
const OPTIONS_H = ROW_H * 2 + 1
const Y_TOGGLES = Y_OPTIONS + OPTIONS_H + pt(11)

export type StartPhoneProps = {
  frame: number
  tapAt: number // play-circle press on the issue view
  sheetAt: number // the start sheet slides up
  flickAt?: { at: number; out: number } // hover flick across the Codex pill
  startAt: number // toolbar Start-coding press → spinner
  collapseAt: number // sheet slides away (start sent)
  glass?: { x: number; y: number }
}

export const StartPhone: React.FC<StartPhoneProps> = ({
  frame,
  tapAt,
  sheetAt,
  flickAt,
  startAt,
  collapseAt,
  glass,
}) => {
  const sheetIn =
    frame < sheetAt
      ? 0
      : spring({ frame: frame - sheetAt, fps: 30, config: SETTLE })
  const sheetOut = interpolate(
    frame,
    [collapseAt, collapseAt + 8],
    [0, 1],
    EASED
  )
  // The spring's overshoot is clamped away: a full-height sheet that sails
  // past its rest position would ride over the status row.
  const sheetY =
    Math.max(0, SHEET_H * (1 - sheetIn)) + SHEET_H * sheetOut
  const flickT = flickAt
    ? interpolate(frame, [flickAt.at, flickAt.at + 4], [0, 1], CLAMP) *
      interpolate(frame, [flickAt.out, flickAt.out + 4], [1, 0], CLAMP)
    : 0
  const starting = frame >= startAt
  const startPress = interpolate(
    frame,
    [startAt, startAt + 3, startAt + 9],
    [0, 1, 0],
    CLAMP
  )
  const scrimO = Math.min(1 - sheetOut, sheetIn) * 0.45
  const toastAt = collapseAt + 6
  const toastT =
    frame < toastAt
      ? 0
      : spring({ frame: frame - toastAt, fps: 30, config: SETTLE })

  return (
    <PhoneChassis glass={glass}>
      {/* the REAL mobile issue detail underneath */}
      <IssueScreen
        frame={frame}
        identifier={CL_ISSUE.id}
        title={CL_ISSUE.title}
        origin="Feedback widget"
        status="backlog"
        statusLabel="Backlog"
        priorityLabel="No priority"
        assignee={{ name: CL_ISSUE.assigneeName, initials: "RC" }}
        due={CL_ISSUE.due}
        labelChip={CL_LABELS.widget}
        description={REPORT.details}
        activity={[
          { text: "Feedback widget created the issue · 12 min ago" },
          { text: "Jamie Lee subscribed as reporter · 12 min ago" },
        ]}
        playPressAt={tapAt}
      />

      {/* scrim under the sheet */}
      {scrimO > 0.01 ? (
        <div
          style={{
            position: "absolute",
            inset: 0,
            backgroundColor: `rgba(0,0,0,${scrimO.toFixed(3)})`,
          }}
        />
      ) : null}

      {/* the start sheet (real StartCodingSheet form) */}
      {frame >= sheetAt && sheetOut < 1 ? (
        <div
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            top: SHEET_TOP,
            height: SHEET_H,
            boxSizing: "border-box",
            borderRadius: `${pt(38)}px ${pt(38)}px 36px 36px`,
            // The sheet is OPAQUE (it carries its own AppBackground ramp) —
            // a translucent panel let the issue detail behind it read through.
            background: `linear-gradient(180deg, #131316, #1b1b1e)`,
            borderTop: `1px solid ${C.strokeCard}`,
            translate: `0px ${sheetY.toFixed(1)}px`,
            fontFamily: UI_FONT,
            letterSpacing: "-0.02em",
            overflow: "hidden",
          }}
        >
          {/* the sheet grabber — the only chrome above the content */}
          <div
            style={{
              position: "absolute",
              left: "50%",
              top: Y_GRABBER,
              width: pt(36),
              height: pt(5),
              borderRadius: 999,
              translate: "-50% 0px",
              backgroundColor: "rgba(255,255,255,0.22)",
            }}
          />

          {/* subject: Issues | Actions | Chat */}
          <SegControl top={Y_TABS} items={TABS} activeId="issues" />

          {/* Issues section header + the grouped picker card */}
          <div
            style={{
              position: "absolute",
              left: pt(39),
              top: Y_HEADER,
              height: HEADER_H,
              display: "flex",
              alignItems: "center",
              fontSize: pt(15),
              fontWeight: 600,
              color: G.header,
            }}
          >
            {PHONE_START.issuesLabel}
          </div>
          <Card top={Y_PICKER}>
            <div
              style={{
                height: ROW_H,
                boxSizing: "border-box",
                display: "flex",
                alignItems: "center",
                gap: pt(8.5),
                padding: `0 ${pt(18.5)}px`,
                fontSize: pt(17),
                color: G.placeholder,
              }}
            >
              <Glyph size={pt(13)} sw={2}>
                <circle cx="11" cy="11" r="8" />
                <path d="m21 21-4.3-4.3" />
              </Glyph>
              {PHONE_START.searchPlaceholder}
            </div>
            <Sep />
            <div style={{ padding: `${LIST_PAD}px 0` }}>
              {SHEET_ISSUES.map((issue) => (
                <IssueRow
                  key={issue.id}
                  issue={issue}
                  checked={issue.id === CHECKED_ID}
                />
              ))}
            </div>
          </Card>

          {/* agent strip, then Model + Effort, then the launch toggles */}
          <SegControl
            top={Y_AGENTS}
            items={AGENTS}
            activeId="claude"
            flick={{ id: "codex", t: flickT }}
          />
          <Card top={Y_OPTIONS}>
            <PickerRow
              label={PHONE_START.modelLabel}
              value={PHONE_START.model}
            />
            <Sep />
            <PickerRow
              label={PHONE_START.effortLabel}
              value={PHONE_START.effort}
            />
          </Card>
          <Card top={Y_TOGGLES}>
            {(["Ultracode", "Plan mode"] as const).map(
              (label, i) => (
                <React.Fragment key={label}>
                  {i > 0 ? <Sep /> : null}
                  <ToggleRow label={label} />
                </React.Fragment>
              )
            )}
          </Card>

          {/* the ONE pinned action (ExpUI GlassSubmitButton): full width at
              the sheet's floor, `primary` fill with `primaryForeground` text
              and no hairline — never a toolbar button (EXP-687). */}
          <div
            style={{
              position: "absolute",
              left: pt(16),
              right: pt(16),
              bottom: pt(30),
              height: pt(48),
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: pt(8),
              borderRadius: pt(10),
              backgroundColor: PRIMARY_FILL,
              color: PRIMARY_FG,
              fontSize: pt(17),
              fontWeight: 500,
              scale: String(1 - 0.02 * startPress),
            }}
          >
            {starting ? <Spinner frame={frame} /> : null}
            {PHONE_START.confirm}
          </div>
        </div>
      ) : null}

      {/* the "Start sent" capsule toast (post-collapse) */}
      {toastT > 0.01 ? (
        <div
          style={{
            position: "absolute",
            left: 14,
            right: 14,
            bottom: 20,
            boxSizing: "border-box",
            borderRadius: 14,
            padding: "9px 12px",
            backgroundColor: C.panelFloat,
            border: `1px solid ${C.strokeCard}`,
            boxShadow: "0 14px 34px rgba(0,0,0,0.45)",
            fontSize: 11.5,
            lineHeight: 1.45,
            color: C.muted,
            textAlign: "center",
            opacity: Math.min(1, toastT * 2),
            translate: `0px ${(1 - toastT) * 18}px`,
          }}
        >
          {PHONE_START.toast}
        </div>
      ) : null}
    </PhoneChassis>
  )
}
