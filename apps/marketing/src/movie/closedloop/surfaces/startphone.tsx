// closedloop/surfaces/startphone.tsx — the remote-start phone (EXP-388,
// matched to the shipping mobile UI): the REAL issue detail screen for
// EXP-151 (identifier pill + origin chip, title, property chip box,
// description, floating bottom bar whose right circle is the icon-only play
// button), and the REAL StartCodingSheet over it — Cancel / Start coding
// toolbar, an Issues section with the search row and the checked issue row,
// the agent pill strip (Claude Code · Codex · pi), Model and Effort picker
// rows. One desktop online = no Device row (like the app); after the start
// the "Start sent to MacBook Pro" capsule toast confirms.
// All frame props are COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, SETTLE, UI_FONT } from "../../ships/theme"
import { ClaudeMark, CodexMark, PiMark } from "./agentmarks"
import { CL_ISSUE, CL_LABELS, PHONE_START, REPORT } from "../fixtures"
import { PhoneChassis } from "./steerphone"
import { Glyph, IssueScreen, MPriorityIcon, MStatusIcon } from "./mobileui"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

const SHEET_H = 470

const Spinner: React.FC<{ frame: number; size?: number }> = ({
  frame,
  size = 12,
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

// Phone twin of the real agent pill strip (selected = solid indigo capsule).
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
            backgroundColor: active
              ? C.indigo
              : flick > 0
                ? `rgba(255,255,255,${0.06 + 0.07 * flick})`
                : "rgba(255,255,255,0.06)",
            color: active ? "#ffffff" : C.muted,
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

// A grouped Form card row: label left, value + chevrons right.
const PickerRow: React.FC<{ label: string; value: string }> = ({
  label,
  value,
}) => (
  <div
    style={{
      height: 40,
      boxSizing: "border-box",
      display: "flex",
      alignItems: "center",
      gap: 8,
      padding: "0 12px",
    }}
  >
    <span style={{ fontSize: 12.5, color: C.text }}>{label}</span>
    <span
      style={{
        marginLeft: "auto",
        display: "flex",
        alignItems: "center",
        gap: 5,
        color: C.muted,
      }}
    >
      <span style={{ fontSize: 12.5 }}>{value}</span>
      <Glyph size={10} sw={2.2}>
        <path d="m7 15 5 5 5-5" />
        <path d="m7 9 5-5 5 5" />
      </Glyph>
    </span>
  </div>
)

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
        status="todo"
        statusLabel="Todo"
        priorityLabel="No priority"
        labelChip={CL_LABELS.widget}
        description={REPORT.details}
        activity={[
          "Feedback widget created the issue · 12 min ago",
          "Jamie Lee subscribed as reporter · 12 min ago",
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
            bottom: 0,
            height: SHEET_H,
            boxSizing: "border-box",
            borderRadius: "24px 24px 36px 36px",
            backgroundColor: C.panelFloat,
            borderTop: `1px solid ${C.strokeCard}`,
            translate: `0px ${sheetY.toFixed(1)}px`,
            fontFamily: UI_FONT,
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

          {/* toolbar: glass capsule Cancel left · Start coding right */}
          <div
            style={{
              position: "absolute",
              left: 16,
              right: 16,
              top: 18,
              display: "flex",
              alignItems: "center",
            }}
          >
            <span
              style={{
                height: 30,
                display: "inline-flex",
                alignItems: "center",
                padding: "0 13px",
                borderRadius: 999,
                backgroundColor: C.fillActive,
                fontSize: 12.5,
                fontWeight: 500,
                color: C.text,
              }}
            >
              {PHONE_START.cancel}
            </span>
            <span
              style={{
                marginLeft: "auto",
                height: 30,
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                padding: "0 13px",
                borderRadius: 999,
                backgroundColor: C.fillActive,
                fontSize: 12.5,
                fontWeight: 600,
                color: C.text,
                scale: String(1 - 0.05 * startPress),
              }}
            >
              {starting ? <Spinner frame={frame} /> : null}
              {PHONE_START.confirm}
            </span>
          </div>

          {/* Issues section: search row + the checked issue row */}
          <div
            style={{
              position: "absolute",
              left: 16,
              top: 60,
              fontSize: 14,
              fontWeight: 600,
              color: C.muted,
            }}
          >
            {PHONE_START.issuesLabel}
          </div>
          <div
            style={{
              position: "absolute",
              left: 16,
              right: 16,
              top: 84,
              borderRadius: 12,
              backgroundColor: C.fillCard,
              border: `1px solid rgba(255,255,255,0.06)`,
              overflow: "hidden",
            }}
          >
            <div
              style={{
                height: 36,
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "0 12px",
                borderBottom: `1px solid rgba(255,255,255,0.06)`,
                color: C.dim,
              }}
            >
              <Glyph size={13} sw={2}>
                <circle cx="11" cy="11" r="8" />
                <path d="m21 21-4.3-4.3" />
              </Glyph>
              <span style={{ fontSize: 12 }}>
                {PHONE_START.searchPlaceholder}
              </span>
            </div>
            <div
              style={{
                height: 40,
                display: "flex",
                alignItems: "center",
                gap: 7,
                padding: "0 12px",
                backgroundColor: "rgba(99,102,241,0.12)",
              }}
            >
              <span style={{ color: C.indigoSoft, display: "flex" }}>
                <Glyph size={14} sw={2}>
                  <circle cx="12" cy="12" r="9" />
                  <path d="m8.5 12 2.5 2.5 5-5" />
                </Glyph>
              </span>
              <MPriorityIcon priority="none" size={12} />
              <span
                style={{
                  fontFamily: MONO_FONT,
                  fontSize: 10,
                  color: C.dim,
                  flexShrink: 0,
                }}
              >
                {CL_ISSUE.id}
              </span>
              <MStatusIcon status="todo" size={12} />
              <span
                style={{
                  fontSize: 12,
                  color: C.text,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {CL_ISSUE.title}
              </span>
            </div>
          </div>

          {/* agent pill strip, then Model + Effort picker rows */}
          <div style={{ position: "absolute", left: 0, right: 0, top: 180 }}>
            <AgentTabs flickT={flickT} />
          </div>
          <div
            style={{
              position: "absolute",
              left: 16,
              right: 16,
              top: 222,
              borderRadius: 12,
              backgroundColor: C.fillCard,
              border: `1px solid rgba(255,255,255,0.06)`,
              overflow: "hidden",
            }}
          >
            <PickerRow
              label={PHONE_START.modelLabel}
              value={PHONE_START.model}
            />
            <div
              style={{ height: 1, backgroundColor: "rgba(255,255,255,0.06)" }}
            />
            <PickerRow
              label={PHONE_START.effortLabel}
              value={PHONE_START.effort}
            />
          </div>

          {/* Claude-only toggles (all off) */}
          <div
            style={{
              position: "absolute",
              left: 16,
              right: 16,
              top: 316,
              borderRadius: 12,
              backgroundColor: C.fillCard,
              border: `1px solid rgba(255,255,255,0.06)`,
              overflow: "hidden",
            }}
          >
            {(["Ultracode", "Plan mode", "Skip permissions"] as const).map(
              (label, i) => (
                <React.Fragment key={label}>
                  {i > 0 ? (
                    <div
                      style={{
                        height: 1,
                        backgroundColor: "rgba(255,255,255,0.06)",
                      }}
                    />
                  ) : null}
                  <div
                    style={{
                      height: 40,
                      boxSizing: "border-box",
                      display: "flex",
                      alignItems: "center",
                      padding: "0 12px",
                    }}
                  >
                    <span style={{ fontSize: 12.5, color: C.text }}>
                      {label}
                    </span>
                    <span
                      style={{
                        marginLeft: "auto",
                        width: 36,
                        height: 22,
                        boxSizing: "border-box",
                        borderRadius: 999,
                        backgroundColor: C.fillActive,
                        padding: 2,
                        display: "flex",
                      }}
                    >
                      <span
                        style={{
                          width: 18,
                          height: 18,
                          borderRadius: 999,
                          backgroundColor: "#e5e5e5",
                        }}
                      />
                    </span>
                  </div>
                </React.Fragment>
              )
            )}
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
