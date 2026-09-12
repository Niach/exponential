// closedloop/surfaces/startphone.tsx — the remote-start phone (EXP-388,
// matched to the shipping mobile UI): the REAL issue detail screen for
// EXP-151 underneath, and the REAL Agent page pushed over it — the ONE
// launcher since EXP-825, which retired the three-tab Start-coding sheet this
// surface used to draw (EXP-845). A pushed detail slides in from the right
// with its inline "Agent" nav bar, the `AgentComposerCard` (ExpUI
// `GlassComposer`: the issue chip the play button put there, the prompt
// field, the `#`/▶/image tool row and the LABELLED primary submit pill
// "Start coding"), the `AgentOptionsRow` pills under it (Device · Agent ·
// Plan · ⋯ — the row scrolls, so Model sits off the right edge) and the
// caller's Past runs. After the start the "Start
// sent to MacBook Pro" capsule toast confirms.
//
// EVERY number in the screen is authored in iOS POINTS on the 414pt canvas and
// scaled ONCE through `pt()` — the same measurements the marketing page's
// sibling recreation carries (src/mobile/AgentComposer.tsx + the `mag-*`
// rules in styles/mobile.css), so the film and the page never drift apart
// into hand-tuned marketing px.
// All frame props are COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, SETTLE, UI_FONT } from "../../ships/theme"
import { ClaudeMark } from "./agentmarks"
import { CL_BOARD, CL_ISSUE, CL_LABELS, PHONE_START, REPORT } from "../fixtures"
import { PHONE, PhoneChassis } from "./steerphone"
import {
  Glyph,
  IssueScreen,
  MStatusIcon,
  type MobileStatus,
} from "./mobileui"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

// iOS points → phone px. The chassis screen is 312px wide for the 414pt
// device, so ONE factor carries every measured value across.
const PT = PHONE.screenW / 414
const pt = (v: number): number => Math.round(v * PT * 10) / 10

// Glass tokens the screen's own controls speak (mobile GlassTheme).
const G = {
  card: "rgba(255,255,255,0.05)",
  cardStroke: "rgba(255,255,255,0.10)",
  pill: "rgba(255,255,255,0.051)",
  pillStroke: "rgba(255,255,255,0.078)",
  chip: "rgba(255,255,255,0.059)",
  sep: "rgba(255,255,255,0.10)",
  header: "rgba(255,255,255,0.66)",
  placeholder: "rgba(255,255,255,0.35)",
  value: "rgba(255,255,255,0.85)",
  chev: "rgba(255,255,255,0.50)",
  id: "rgba(255,255,255,0.62)",
  tool: "rgba(255,255,255,0.70)",
  caption: "rgba(255,255,255,0.50)",
} as const

// DesignTokens.Palette.primary / primaryForeground — the submit pill's fill
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

// ── The chipped subject + the caller's past runs ─────────────────────────────
// A play button navigates here with ITS issue chipped; the rest of the board
// is one `#` tap away, so the composer shows exactly one chip.
type PhoneIssue = {
  id: string
  title: string
  status: MobileStatus
}
const CHIPPED: PhoneIssue = (() => {
  const row = CL_BOARD.find((issue) => issue.id === CL_ISSUE.id)!
  return { id: row.id, title: row.title, status: row.status }
})()
// AgentSessionsList under the composer: the finished runs, newest first.
const PAST = CL_BOARD.filter(
  (row) => row.status !== "backlog" && row.id !== CL_ISSUE.id
).slice(0, 3)

// ── Layout atoms ────────────────────────────────────────────────────────────
const INSET = pt(18.5)
const CARD_RADIUS = pt(24.5)

const Card: React.FC<{ top: number; children: React.ReactNode }> = ({
  top,
  children,
}) => (
  <div
    style={{
      position: "absolute",
      left: INSET,
      right: INSET,
      top,
      borderRadius: CARD_RADIUS,
      backgroundColor: G.card,
      border: `1px solid ${G.cardStroke}`,
      overflow: "hidden",
    }}
  >
    {children}
  </div>
)

// One muted glass pill of the options row; `hot` is the hover flick.
const OptionPill: React.FC<{
  children: React.ReactNode
  chevron?: boolean
  hot?: number
}> = ({ children, chevron = true, hot = 0 }) => (
  <span
    style={{
      display: "flex",
      alignItems: "center",
      gap: pt(5),
      height: pt(30),
      padding: `0 ${pt(10)}px`,
      borderRadius: 999,
      backgroundColor:
        hot > 0 ? `rgba(255,255,255,${(0.051 + 0.05 * hot).toFixed(3)})` : G.pill,
      border: `1px solid ${G.pillStroke}`,
      fontSize: pt(13),
      color: G.value,
      whiteSpace: "nowrap",
    }}
  >
    {children}
    {chevron ? (
      <span style={{ color: G.chev, display: "flex" }}>
        <Glyph size={pt(11)} sw={2}>
          <path d="m6 9 6 6 6-6" />
        </Glyph>
      </span>
    ) : null}
  </span>
)

// iOS mini switch, ON: the white capsule knob parked right on a primary track.
const PlanSwitch: React.FC = () => (
  <span
    style={{
      position: "relative",
      width: pt(28),
      height: pt(17),
      borderRadius: 999,
      backgroundColor: PRIMARY_FILL,
      flexShrink: 0,
    }}
  >
    <span
      style={{
        position: "absolute",
        right: pt(2),
        top: pt(2),
        width: pt(13),
        height: pt(13),
        borderRadius: 999,
        backgroundColor: PRIMARY_FG,
      }}
    />
  </span>
)

// ── Screen layout (screen-local Ys, all derived — nothing hand-placed) ───────
// The status row the chassis paints stays clear: content starts under the
// 47pt safe area, like every pushed iOS detail.
const SAFE_TOP = pt(47)
const NAV_H = pt(44)
const Y_CARD = SAFE_TOP + NAV_H + pt(12)
const CHIP_BAND = pt(12) + pt(30)
const FIELD_H = pt(14) + pt(44)
const TOOLS_H = pt(10) + pt(34) + pt(12)
const CARD_H = CHIP_BAND + FIELD_H + TOOLS_H
const Y_OPTIONS = Y_CARD + CARD_H + pt(12)
const Y_PAST_LABEL = Y_OPTIONS + pt(30) + pt(20)
const Y_PAST = Y_PAST_LABEL + pt(15) + pt(8.5)
const ROW_H = pt(56)

export type StartPhoneProps = {
  frame: number
  tapAt: number // play-circle press on the issue view
  sheetAt: number // the Agent page pushes in
  flickAt?: { at: number; out: number } // hover flick across the Agent pill
  startAt: number // submit press → spinner
  collapseAt: number // the screen pops back (start sent)
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
  // A pushed detail slides in from the RIGHT and pops back the same way.
  const pushIn =
    frame < sheetAt
      ? 0
      : spring({ frame: frame - sheetAt, fps: 30, config: SETTLE })
  const pushOut = interpolate(frame, [collapseAt, collapseAt + 8], [0, 1], EASED)
  const pushX =
    Math.max(0, PHONE.screenW * (1 - pushIn)) + PHONE.screenW * pushOut
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

      {/* the pushed Agent page (real AgentPageView form) */}
      {frame >= sheetAt && pushOut < 1 ? (
        <div
          style={{
            position: "absolute",
            inset: 0,
            boxSizing: "border-box",
            // The screen is OPAQUE (it carries its own AppBackground ramp) —
            // a translucent panel let the issue detail behind it read through.
            background: `linear-gradient(180deg, #131316, #1b1b1e)`,
            translate: `${pushX.toFixed(1)}px 0px`,
            fontFamily: UI_FONT,
            letterSpacing: "-0.02em",
            overflow: "hidden",
          }}
        >
          {/* the inline nav bar — native back, no tab bar on a detail */}
          <div
            style={{
              position: "absolute",
              left: pt(12),
              right: pt(12),
              top: SAFE_TOP,
              height: NAV_H,
              display: "flex",
              alignItems: "center",
              gap: pt(6),
              color: C.text,
            }}
          >
            <span style={{ display: "flex", rotate: "180deg" }}>
              <Glyph size={pt(17)} sw={2.2}>
                <path d="m9 18 6-6-6-6" />
              </Glyph>
            </span>
            <span style={{ fontSize: pt(17), fontWeight: 600 }}>
              {PHONE_START.navTitle}
            </span>
          </div>

          {/* AgentComposerCard: chip · field · tools + the labelled submit */}
          <Card top={Y_CARD}>
            <div style={{ display: "flex", padding: `${pt(12)}px ${pt(12)}px 0` }}>
              <span
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: pt(5),
                  maxWidth: "100%",
                  height: pt(30),
                  padding: `0 ${pt(6)}px 0 ${pt(10)}px`,
                  borderRadius: 999,
                  backgroundColor: G.chip,
                  border: `1px solid ${G.cardStroke}`,
                  fontSize: pt(15),
                  color: C.text,
                }}
              >
                <MStatusIcon status={CHIPPED.status} size={pt(15)} />
                <span
                  style={{
                    fontFamily: MONO_FONT,
                    fontSize: pt(12),
                    color: G.id,
                    flexShrink: 0,
                  }}
                >
                  {CHIPPED.id}
                </span>
                <span
                  style={{
                    overflow: "hidden",
                    whiteSpace: "nowrap",
                    textOverflow: "ellipsis",
                  }}
                >
                  {CHIPPED.title}
                </span>
                <span style={{ color: G.chev, display: "flex" }}>
                  <Glyph size={pt(11)} sw={2}>
                    <path d="M18 6 6 18M6 6l12 12" />
                  </Glyph>
                </span>
              </span>
            </div>
            <div
              style={{
                minHeight: pt(44),
                padding: `${pt(14)}px ${pt(12)}px 0`,
                fontSize: pt(17),
                color: G.placeholder,
              }}
            >
              {PHONE_START.placeholder}
            </div>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: pt(18),
                padding: `${pt(10)}px ${pt(12)}px ${pt(12)}px`,
                color: G.tool,
              }}
            >
              {/* `#` issues · ▶ actions · the image glyph, every client */}
              <Glyph size={pt(17)} sw={2}>
                <path d="M4 9h16M4 15h16M10 3 8 21M16 3l-2 18" />
              </Glyph>
              <Glyph size={pt(17)} sw={2}>
                <path d="m6 3 14 9-14 9V3z" />
              </Glyph>
              <Glyph size={pt(17)} sw={2}>
                <rect x="3" y="3" width="18" height="18" rx="2" />
                <circle cx="9" cy="9" r="2" />
                <path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21" />
              </Glyph>
              <span style={{ flex: 1 }} />
              {/* ExpUI GlassPill, primary: the submit is LABELLED on mobile */}
              <span
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: pt(6),
                  height: pt(34),
                  padding: `0 ${pt(14)}px`,
                  borderRadius: 999,
                  backgroundColor: PRIMARY_FILL,
                  color: PRIMARY_FG,
                  fontSize: pt(15),
                  fontWeight: 500,
                  whiteSpace: "nowrap",
                  scale: String(1 - 0.02 * startPress),
                }}
              >
                {starting ? <Spinner frame={frame} /> : null}
                {PHONE_START.confirm}
              </span>
            </div>
          </Card>

          {/* AgentOptionsRow: one muted line of glass pills — the phone's
              row SCROLLS, so Model and the rest sit off its right edge */}
          <div
            style={{
              position: "absolute",
              left: INSET,
              right: INSET,
              top: Y_OPTIONS,
              display: "flex",
              alignItems: "center",
              gap: pt(8),
            }}
          >
            <OptionPill>{PHONE_START.device}</OptionPill>
            <OptionPill hot={flickT}>
              <ClaudeMark size={pt(13)} />
              {PHONE_START.agent}
            </OptionPill>
            <span
              style={{
                display: "flex",
                alignItems: "center",
                gap: pt(5),
                height: pt(30),
                padding: `0 ${pt(10)}px`,
                borderRadius: 999,
                backgroundColor: G.pill,
                border: `1px solid ${G.pillStroke}`,
                fontSize: pt(13),
                color: G.value,
              }}
            >
              {PHONE_START.planLabel}
              <PlanSwitch />
            </span>
            {/* `⋯` unfolds Effort, Ultracode, MCP servers and Account. */}
            <OptionPill chevron={false}>
              <Glyph size={pt(14)} sw={2.2}>
                <circle cx="5" cy="12" r="0.8" fill="currentColor" />
                <circle cx="12" cy="12" r="0.8" fill="currentColor" />
                <circle cx="19" cy="12" r="0.8" fill="currentColor" />
              </Glyph>
            </OptionPill>
          </div>

          {/* AgentSessionsList: the caller's finished runs */}
          <div
            style={{
              position: "absolute",
              left: INSET,
              top: Y_PAST_LABEL,
              height: pt(15),
              display: "flex",
              alignItems: "center",
              fontSize: pt(15),
              fontWeight: 600,
              color: G.header,
            }}
          >
            {PHONE_START.pastLabel}
          </div>
          <Card top={Y_PAST}>
            {PAST.map((row, i) => (
              <React.Fragment key={row.id}>
                {i > 0 ? (
                  <div style={{ height: 1, backgroundColor: G.sep }} />
                ) : null}
                <div
                  style={{
                    height: ROW_H,
                    boxSizing: "border-box",
                    display: "flex",
                    alignItems: "center",
                    gap: pt(10),
                    padding: `0 ${pt(18.5)}px`,
                    color: C.text,
                  }}
                >
                  <ClaudeMark size={pt(15)} />
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      display: "flex",
                      flexDirection: "column",
                      gap: pt(5),
                    }}
                  >
                    <span
                      style={{
                        fontSize: pt(15),
                        overflow: "hidden",
                        whiteSpace: "nowrap",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {row.title}
                    </span>
                    <span style={{ fontSize: pt(12), color: G.caption }}>
                      {PHONE_START.pastCaption}
                    </span>
                  </span>
                  <span
                    style={{
                      fontFamily: MONO_FONT,
                      fontSize: pt(12),
                      color: G.id,
                    }}
                  >
                    {row.id}
                  </span>
                </div>
              </React.Fragment>
            ))}
          </Card>
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
