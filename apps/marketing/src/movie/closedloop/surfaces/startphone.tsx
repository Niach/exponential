// closedloop/surfaces/startphone.tsx — the remote start from a phone, as the
// iOS app does it today (IssueScreen.swift `openComposer`, AgentPageView,
// AgentComposerCard, AgentOptionsRow; shots/chat-from-issue/ios.webp):
// the issue page's Start circle PUSHES the Agent page with the issue picked —
// back circle + centered "Agent", the column PINNED TO THE BOTTOM: the
// headline ("Implement" + the issue chip), the composer card ("Additional
// instructions (optional)…", the "+" as its ONLY tool, the round
// circle-arrow-up send glyph — icon-only ×4, EXP-827) and the options line
// (Device · Account · Model · Plan pills). The send turns the glyph into the
// pending spinner, the card leads with the starting row ("Starting on …" +
// the ticking seconds, EXP-1271) and the page OPENS THE NEW THREAD at once
// (pushed from the right) — no toast, no pop back.
//
// EVERY number is authored in iOS POINTS on the 414pt canvas and scaled ONCE
// through `pt()`. All frame props are COMPOSITION-LOCAL to the segment.

import React from "react"
import { interpolate, spring } from "remotion"
import { MONO_FONT, SETTLE, UI_FONT } from "../../ships/theme"
import { ClaudeMark } from "./agentmarks"
import { CL, CL_BOARD, CL_ISSUE, CL_LABELS, REPORT } from "../fixtures"
import { PHONE, PhoneChassis, ScreenBackdrop, ThreadScreen } from "./steerphone"
import {
  BackGlyph,
  Glyph,
  HAIRLINE,
  IOS,
  IssueScreen,
  MStatusIcon,
  NavCircle,
  pt,
} from "./mobileui"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// The movie's launch picks (the options line names them).
const DEVICE = CL.machine // "Riley's MacBook Pro"
const ACCOUNT = "riley@acme.shop"
const MODEL = "Opus"

const CHIPPED = (() => {
  const row = CL_BOARD.find((issue) => issue.id === CL_ISSUE.id)!
  return { id: row.id, title: row.title, status: row.status }
})()

const Spinner: React.FC<{ frame: number; size: number }> = ({ frame, size }) => (
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

// OptionPillLabel: 28pt capsule, caption-medium at the secondary rung, the
// 12pt leading glyph, the 10pt chevron at tertiary; fillRow + strokeCard.
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
      height: pt(28),
      padding: `0 ${pt(10)}px`,
      borderRadius: 999,
      backgroundColor:
        hot > 0 ? `rgba(255,255,255,${(0.0588 + 0.05 * hot).toFixed(3)})` : IOS.fillRow,
      boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
      fontSize: pt(12),
      fontWeight: 500,
      color: IOS.secondary,
      whiteSpace: "nowrap",
      flexShrink: 0,
    }}
  >
    {children}
    {chevron ? (
      <span style={{ color: IOS.tertiary, display: "flex" }}>
        <Glyph size={pt(10)} sw={2.4}>
          <path d="m6 9 6 6 6-6" />
        </Glyph>
      </span>
    ) : null}
  </span>
)

// ── Column geometry (pt, measured off chat-from-issue/ios.webp) ──────────────
// Pinned to the bottom: the options line ends 58pt above the screen's bottom
// edge (16pt padding + the 34pt home-indicator inset + the row's own slack);
// card 92pt (44pt field + the 48pt tool row), the headline 28pt, 12pt between them.
const OPTIONS_BOTTOM = pt(58)
const OPTIONS_H = pt(28)
const CARD_H = pt(92)
const HEAD_H = pt(28)
const GAP = pt(12)

export type StartPhoneProps = {
  frame: number
  tapAt: number // Start-circle press on the issue page
  sheetAt: number // the Agent page pushes in
  flickAt?: { at: number; out: number } // hover flick across the Account pill
  startAt: number // the send press → pending
  collapseAt: number // the send answered: the new thread opens
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
  const W = PHONE.screenW
  const pushIn =
    frame < sheetAt ? 0 : spring({ frame: frame - sheetAt, fps: 30, config: SETTLE })
  const pushX = Math.max(0, W * (1 - pushIn))
  const threadIn =
    frame < collapseAt ? 0 : spring({ frame: frame - collapseAt, fps: 30, config: SETTLE })
  const threadX = Math.max(0, W * (1 - threadIn))
  // the page under a push slides a third of the way left (UINavigationController)
  const agentShift = -W * 0.3 * threadIn
  const issueShift = -W * 0.3 * pushIn
  const flickT = flickAt
    ? interpolate(frame, [flickAt.at, flickAt.at + 4], [0, 1], CLAMP) *
      interpolate(frame, [flickAt.out, flickAt.out + 4], [1, 0], CLAMP)
    : 0
  const sending = frame >= startAt
  const pending = frame >= startAt + 5 // the 150ms pending delay
  const sendPress = interpolate(frame, [startAt, startAt + 3, startAt + 9], [0, 1, 0], CLAMP)
  const elapsed = Math.max(0, (frame - startAt) / 30)

  const cardTop = -OPTIONS_BOTTOM - OPTIONS_H - GAP - CARD_H - (pending ? pt(32) : 0)

  return (
    <PhoneChassis glass={glass}>
      {/* the issue page underneath (before any thread holds the issue) */}
      <div style={{ position: "absolute", inset: 0, translate: `${issueShift.toFixed(1)}px 0px` }}>
        <IssueScreen
          frame={frame}
          identifier={CL_ISSUE.id}
          title={CL_ISSUE.title}
          origin="Feedback widget"
          status="backlog"
          statusLabel="Backlog"
          priorityLabel="No priority"
          assignee={{ name: CL_ISSUE.assigneeName, initials: CL.initials }}
          due={CL_ISSUE.due}
          labelChip={CL_LABELS.widget}
          description={REPORT.details}
          activity={[
            { text: "Feedback widget created the issue · 12 min ago" },
            { text: "Jamie Lee subscribed as reporter · 12 min ago" },
          ]}
          playPressAt={tapAt}
        />
      </div>

      {/* the pushed Agent page */}
      {frame >= sheetAt && threadIn < 1 ? (
        <div
          style={{
            position: "absolute",
            inset: 0,
            translate: `${(pushX + agentShift).toFixed(1)}px 0px`,
            boxShadow: "-12px 0 30px rgba(0,0,0,0.35)",
            fontFamily: UI_FONT,
            overflow: "hidden",
          }}
        >
          <ScreenBackdrop glass={glass} />
          {/* nav bar: back circle · centered "Agent" */}
          <div
            style={{
              position: "absolute",
              top: pt(79) - pt(20.5),
              left: pt(19),
              right: pt(19),
              height: pt(41),
              display: "flex",
              alignItems: "center",
            }}
          >
            <NavCircle>
              <BackGlyph />
            </NavCircle>
            <span
              style={{
                position: "absolute",
                left: 0,
                right: 0,
                textAlign: "center",
                fontSize: pt(17),
                fontWeight: 600,
                color: "#ffffff",
              }}
            >
              Agent
            </span>
          </div>

          {/* the bottom-pinned column */}
          <div
            style={{
              position: "absolute",
              left: pt(16),
              right: 0,
              bottom: -cardTop + GAP,
              height: HEAD_H,
              display: "flex",
              alignItems: "center",
              gap: pt(8),
              overflow: "hidden",
            }}
          >
            <span style={{ fontSize: pt(22), fontWeight: 600, color: "#ffffff", flexShrink: 0 }}>
              Implement
            </span>
            {/* the shared issue badge: status · mono identifier · title · ✕ */}
            <span
              style={{
                display: "flex",
                alignItems: "center",
                gap: pt(5),
                height: pt(24),
                padding: `0 ${pt(6)}px`,
                borderRadius: pt(5),
                backgroundColor: "rgba(255,255,255,0.08)",
                boxShadow: `inset 0 0 0 ${HAIRLINE}px rgba(255,255,255,0.16)`,
                whiteSpace: "nowrap",
                flexShrink: 0,
              }}
            >
              <MStatusIcon status={CHIPPED.status} size={pt(13)} />
              <span style={{ fontFamily: MONO_FONT, fontSize: pt(15), color: IOS.tertiary }}>
                {CHIPPED.id}
              </span>
              <span style={{ fontSize: pt(15), color: "#ffffff" }}>{CHIPPED.title}</span>
              <span style={{ color: IOS.tertiary, display: "flex" }}>
                <Glyph size={pt(11)} sw={2.2}>
                  <path d="M18 6 6 18M6 6l12 12" />
                </Glyph>
              </span>
            </span>
          </div>

          {/* AgentComposerCard on GlassComposer: [starting row] · field · tools */}
          <div
            style={{
              position: "absolute",
              left: pt(16),
              right: pt(16),
              bottom: OPTIONS_BOTTOM + OPTIONS_H + GAP,
              boxSizing: "border-box",
              borderRadius: pt(16),
              backgroundColor: IOS.opaqueCard,
              boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
            }}
          >
            {pending ? (
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: pt(8),
                  padding: `${pt(12)}px ${pt(12)}px 0`,
                }}
              >
                <Spinner frame={frame} size={pt(14)} />
                <span
                  style={{
                    flex: 1,
                    fontSize: pt(15),
                    color: "#ffffff",
                    whiteSpace: "nowrap",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                  }}
                >
                  {`Starting on ${DEVICE}`}
                </span>
                <span
                  style={{
                    fontSize: pt(12),
                    color: IOS.tertiary,
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  {`${elapsed.toFixed(1)} s`}
                </span>
              </div>
            ) : null}
            <div
              style={{
                minHeight: pt(44),
                boxSizing: "border-box",
                padding: `${pt(12)}px ${pt(12)}px 0`,
                fontSize: pt(17),
                color: IOS.tertiary,
              }}
            >
              Additional instructions (optional)…
            </div>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                padding: `${pt(2)}px ${pt(12)}px ${pt(10)}px`,
                color: IOS.secondary,
              }}
            >
              <span
                style={{
                  width: pt(36),
                  height: pt(36),
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <Glyph size={pt(17)} sw={2}>
                  <path d="M5 12h14" />
                  <path d="M12 5v14" />
                </Glyph>
              </span>
              <span style={{ flex: 1 }} />
              <span
                style={{
                  width: pt(28),
                  height: pt(28),
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  color: "#ffffff",
                  scale: String(1 - 0.08 * sendPress),
                }}
              >
                {pending ? (
                  <span style={{ color: IOS.secondary, display: "flex" }}>
                    <Spinner frame={frame} size={pt(14)} />
                  </span>
                ) : (
                  <Glyph size={pt(28)} sw={1.6}>
                    <circle cx="12" cy="12" r="10" />
                    <path d="m16 12-4-4-4 4" />
                    <path d="M12 16V8" />
                  </Glyph>
                )}
              </span>
            </div>
          </div>

          {/* AgentOptionsRow: Device · Account · Model · Plan (the line
              scrolls; Plan runs off the right edge on a phone) */}
          <div
            style={{
              position: "absolute",
              left: pt(18),
              right: 0,
              bottom: OPTIONS_BOTTOM,
              height: OPTIONS_H,
              display: "flex",
              alignItems: "center",
              gap: pt(8),
              overflow: "hidden",
              opacity: sending ? 0.6 : 1,
            }}
          >
            <OptionPill chevron={false}>
              <Glyph size={pt(12)} sw={2}>
                <rect x="2" y="3" width="20" height="14" rx="2" />
                <path d="M8 21h8" />
                <path d="M12 17v4" />
              </Glyph>
              {DEVICE}
            </OptionPill>
            <OptionPill hot={flickT}>
              <ClaudeMark size={pt(12)} />
              {ACCOUNT}
            </OptionPill>
            <OptionPill>{MODEL}</OptionPill>
            <OptionPill chevron={false}>
              Plan
              <span
                style={{
                  position: "relative",
                  width: pt(26),
                  height: pt(16),
                  borderRadius: 999,
                  backgroundColor: "rgba(120,120,128,0.32)",
                }}
              >
                <span
                  style={{
                    position: "absolute",
                    left: pt(2),
                    top: pt(2),
                    width: pt(12),
                    height: pt(12),
                    borderRadius: 999,
                    backgroundColor: "#ffffff",
                  }}
                />
              </span>
            </OptionPill>
          </div>
        </div>
      ) : null}

      {/* the send answered: the page opens the NEW THREAD (pushed) */}
      {frame >= collapseAt ? (
        <div
          style={{
            position: "absolute",
            inset: 0,
            translate: `${threadX.toFixed(1)}px 0px`,
            boxShadow: "-12px 0 30px rgba(0,0,0,0.35)",
          }}
        >
          <ScreenBackdrop glass={glass} />
          <ThreadScreen
            frame={frame}
            feedSchedule={[9999, 9999, 9999]}
            typeAt={9999}
            sendAt={99999}
          />
        </div>
      ) : null}
    </PhoneChassis>
  )
}
