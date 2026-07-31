// closedloop/surfaces/startphone.tsx — the remote-start phone (EXP-385): the
// mobile issue view for EXP-151 with its Start-coding button, and the start
// sheet that slides up over it — agent tabs (Claude Code active · Codex · pi),
// the online device row (the steer rails advertise the desktop), the model
// row, and the primary Start button that flips to "Starting…". The sheet IS
// the film's start-coding dialog now, so the beats give it a long dwell.
// All frame props are COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, SETTLE } from "../../ships/theme"
import { ClaudeMark, CodexMark, PiMark } from "./agentmarks"
import { CL, CL_ISSUE, PHONE_START, REPORT } from "../fixtures"
import { PhoneChassis } from "./steerphone"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

const SHEET_H = 344

const Glyph: React.FC<{
  size: number
  sw?: number
  children: React.ReactNode
}> = ({ size, sw = 1.9, children }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={sw}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: "block", flexShrink: 0 }}
  >
    {children}
  </svg>
)

const Spinner: React.FC<{ frame: number; size?: number }> = ({
  frame,
  size = 13,
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

// Phone-sized twin of the desktop dialog's agent tab strip.
const AGENT_TABS = [
  { id: "claude", label: "Claude Code", Mark: ClaudeMark, active: true },
  { id: "codex", label: "Codex", Mark: CodexMark, active: false },
  { id: "pi", label: "pi", Mark: PiMark, active: false },
] as const

const AgentTabs: React.FC<{ flickT: number }> = ({ flickT }) => (
  <div style={{ display: "flex", justifyContent: "center", gap: 5 }}>
    {AGENT_TABS.map(({ id, label, Mark, active }) => {
      const flick = id === "codex" ? flickT : 0
      return (
        <div
          key={id}
          style={{
            height: 28,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "0 10px",
            borderRadius: 999,
            border: `1px solid ${active ? C.strokeActive : "transparent"}`,
            backgroundColor: active
              ? C.fillActive
              : flick > 0
                ? `rgba(255,255,255,${0.07 * flick})`
                : "transparent",
            color: active ? C.text : C.muted,
          }}
        >
          <Mark size={11} />
          <span style={{ fontSize: 11.5, fontWeight: 500, lineHeight: "14px" }}>
            {label}
          </span>
        </div>
      )
    })}
  </div>
)

const FieldBox: React.FC<{
  label: string
  y: number
  children: React.ReactNode
}> = ({ label, y, children }) => (
  <>
    <div
      style={{
        position: "absolute",
        left: 16,
        top: y,
        fontSize: 11,
        fontWeight: 600,
        letterSpacing: 0.3,
        textTransform: "uppercase",
        color: C.dim,
      }}
    >
      {label}
    </div>
    <div
      style={{
        position: "absolute",
        left: 16,
        right: 16,
        top: y + 18,
        height: 38,
        boxSizing: "border-box",
        borderRadius: 10,
        border: `1px solid ${C.strokeCard}`,
        backgroundColor: C.fillCard,
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "0 12px",
      }}
    >
      {children}
      <span style={{ marginLeft: "auto", color: C.dim, display: "flex" }}>
        <Glyph size={11} sw={2.2}>
          <path d="m6 9 6 6 6-6" />
        </Glyph>
      </span>
    </div>
  </>
)

export type StartPhoneProps = {
  frame: number
  tapAt: number // Start-coding button press on the issue view
  sheetAt: number // the start sheet slides up
  flickAt?: { at: number; out: number } // hover flick across the Codex tab
  startAt: number // sheet Start press → "Starting…"
  collapseAt: number // sheet slides away (session spawned)
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
  const tapT = interpolate(frame, [tapAt, tapAt + 3, tapAt + 9], [0, 1, 0], CLAMP)
  const sheetIn =
    frame < sheetAt ? 0 : spring({ frame: frame - sheetAt, fps: 30, config: SETTLE })
  const sheetOut = interpolate(
    frame,
    [collapseAt, collapseAt + 8],
    [0, 1],
    EASED
  )
  const sheetY = SHEET_H * (1 - sheetIn) + SHEET_H * sheetOut
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

  return (
    <PhoneChassis glass={glass}>
      {/* board header */}
      <div
        style={{
          position: "absolute",
          top: 44,
          left: 0,
          right: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 6,
        }}
      >
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: 3,
            backgroundColor: CL.projectColor,
          }}
        />
        <span style={{ fontSize: 15, fontWeight: 600, color: C.text }}>
          {CL.project}
        </span>
      </div>

      {/* the issue card */}
      <div
        style={{
          position: "absolute",
          left: 14,
          right: 14,
          top: 86,
          boxSizing: "border-box",
          borderRadius: 16,
          border: `1px solid ${C.strokeCard}`,
          backgroundColor: C.fillCard,
          padding: "14px 14px 16px",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{ fontFamily: MONO_FONT, fontSize: 11, color: C.muted }}
          >
            {CL_ISSUE.id}
          </span>
          <span
            style={{
              marginLeft: "auto",
              display: "flex",
              alignItems: "center",
              gap: 5,
              height: 22,
              padding: "0 9px",
              borderRadius: 999,
              backgroundColor: C.fillActive,
              color: C.text,
              fontSize: 11,
              fontWeight: 500,
            }}
          >
            <span
              style={{
                width: 7,
                height: 7,
                borderRadius: 999,
                border: `1.6px solid ${C.statusTodo}`,
                boxSizing: "border-box",
              }}
            />
            {PHONE_START.status}
          </span>
        </div>
        <div
          style={{
            marginTop: 9,
            fontSize: 15,
            fontWeight: 600,
            lineHeight: 1.3,
            color: C.text,
          }}
        >
          {CL_ISSUE.title}
        </div>
        <div
          style={{
            marginTop: 7,
            fontSize: 12,
            lineHeight: 1.5,
            color: C.muted,
          }}
        >
          {REPORT.details}
        </div>
      </div>

      {/* Start coding (issue view) */}
      <div
        style={{
          position: "absolute",
          left: 14,
          right: 14,
          top: 262,
          height: 44,
          borderRadius: 12,
          backgroundColor: C.indigo,
          color: "#ffffff",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 8,
          fontSize: 13.5,
          fontWeight: 600,
          scale: String(1 - 0.03 * tapT),
          filter: tapT > 0 ? `brightness(${1 + 0.12 * tapT})` : undefined,
        }}
      >
        <Glyph size={13} sw={2.2}>
          <path d="M7 5.5 18.5 12 7 18.5Z" />
        </Glyph>
        {PHONE_START.button}
      </div>

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

      {/* the start sheet */}
      {frame >= sheetAt && sheetOut < 1 ? (
        <div
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            bottom: 0,
            height: SHEET_H,
            boxSizing: "border-box",
            borderRadius: "24px 24px 36px 36px",
            backgroundColor: C.panelFloat,
            borderTop: `1px solid ${C.strokeCard}`,
            translate: `0px ${sheetY.toFixed(1)}px`,
          }}
        >
          {/* grabber */}
          <div
            style={{
              position: "absolute",
              top: 8,
              left: "50%",
              width: 36,
              height: 4,
              marginLeft: -18,
              borderRadius: 999,
              backgroundColor: C.fillActive,
            }}
          />
          <div
            style={{
              position: "absolute",
              left: 16,
              top: 22,
              fontSize: 15,
              fontWeight: 600,
              color: C.text,
            }}
          >
            {PHONE_START.sheetTitle}
          </div>

          <div style={{ position: "absolute", left: 0, right: 0, top: 52 }}>
            <AgentTabs flickT={flickT} />
          </div>

          <FieldBox label={PHONE_START.deviceLabel} y={98}>
            <span
              style={{
                width: 7,
                height: 7,
                borderRadius: 999,
                backgroundColor: C.green,
                flexShrink: 0,
              }}
            />
            <span style={{ fontSize: 12.5, color: C.text }}>
              {PHONE_START.device}
            </span>
            <span style={{ fontSize: 11.5, color: C.muted }}>
              {PHONE_START.deviceState}
            </span>
          </FieldBox>

          <FieldBox label={PHONE_START.modelLabel} y={168}>
            <span style={{ fontSize: 12.5, color: C.text }}>
              {PHONE_START.model}
            </span>
          </FieldBox>

          {/* primary Start button */}
          <div
            style={{
              position: "absolute",
              left: 16,
              right: 16,
              top: 244,
              height: 44,
              borderRadius: 12,
              backgroundColor: C.primary,
              color: C.primaryFg,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: 8,
              fontSize: 13.5,
              fontWeight: 600,
              scale: String(1 - 0.03 * startPress),
            }}
          >
            {starting ? (
              <>
                <Spinner frame={frame} />
                {PHONE_START.starting}
              </>
            ) : (
              PHONE_START.button
            )}
          </div>
        </div>
      ) : null}
    </PhoneChassis>
  )
}
