// closedloop/surfaces/steerphone.tsx — the iPhone chassis + the live THREAD
// screen (shots/steering/ios.webp + shots/thread/ios.webp, ThreadScreen /
// AgentSessionView after N-12 THE Dock and EXP-1334's thread header): back
// circle · the thread's real name · the red Stop pill; THE strip (+ ·
// Conversation · the issue chip · Guide); ONE transcript of Trace lines,
// prose and the edited-files card; and the floating composer (the usage-ring
// circle + "Type / for commands", opening into the ONE card row: + · device ·
// model · Plan · ring · send). All frame props are COMPOSITION-LOCAL.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, GLASS, MONO_FONT, POP, UI_FONT } from "../../ships/theme"
import type { SteerItem } from "../../ships/fixtures"
import { typed, useBlink, wallpaperBackground } from "../../ships/rig"
import {
  CL,
  CL_DIFF_FILES,
  CL_DIFF_ROWS,
  CL_FILE_STATS,
  CL_ISSUE,
  CL_PHONE_FEED,
  CL_STEER_MSG,
} from "../fixtures"
import {
  BackGlyph,
  BarCircle,
  DiffFileCard,
  Glyph,
  HAIRLINE,
  IOS,
  MStatusIcon,
  MoreGlyph,
  NavCircle,
  pt,
  type MobileStatus,
} from "./mobileui"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export const PHONE = { w: 330, screenW: 312, screenH: 660 } as const
export const PHONE_TOTAL_H = PHONE.screenH + 18 // screen + bezel padding

// The screen's backdrop (AppBackground over the glass recipe) — exported so
// a PUSHED page (StartPhone's Agent page and new thread) paints the very same
// opaque ground the chassis does, and a cross-fade between them never shows.
export const ScreenBackdrop: React.FC<{ glass?: { x: number; y: number } }> = ({
  glass,
}) =>
  glass ? (
    <div style={{ position: "absolute", inset: 0, backgroundColor: C.canvas }}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          background: wallpaperBackground(-(glass.x + 9), -(glass.y + 9)),
        }}
      />
      <div
        style={{
          position: "absolute",
          inset: 0,
          background: `linear-gradient(to bottom, rgba(5,5,7,${GLASS.phoneAlpha}), rgba(17,17,20,${GLASS.phoneAlpha}))`,
        }}
      />
    </div>
  ) : (
    <div
      style={{
        position: "absolute",
        inset: 0,
        background: `linear-gradient(to bottom, #050507, #111114)`,
      }}
    />
  )

// ── The bare chassis: bezel, screen, status row, dynamic island ──────────────
// `glass` = the chassis' comp position: the screen then renders the EXP-359
// glass recipe (wallpaper replica + page gradient at the mobile alpha) instead
// of the solid gradient, matching the IDE window. The replica offset ignores
// any phone scale transform — on the soft wallpaper gradient the slight zoom
// is invisible, and a real backdrop-filter is off-limits (see rig.tsx).
export const PhoneChassis: React.FC<{
  children: React.ReactNode
  glass?: { x: number; y: number }
  hideStatus?: boolean // lock-screen shots carry their own big clock
}> = ({ children, glass, hideStatus }) => (
  <div
    style={{
      width: PHONE.w,
      padding: 9,
      boxSizing: "border-box",
      borderRadius: 44,
      backgroundColor: "#050505",
      border: "1px solid rgba(255,255,255,0.14)",
      boxShadow: "0 30px 70px rgba(0,0,0,0.55), 0 4px 18px rgba(0,0,0,0.4)",
      fontFamily: UI_FONT,
    }}
  >
    <div
      style={{
        position: "relative",
        width: PHONE.screenW,
        height: PHONE.screenH,
        borderRadius: 36,
        overflow: "hidden",
        backgroundColor: glass ? C.canvas : undefined,
        background: glass
          ? undefined
          : `linear-gradient(to bottom, #050507, #111114)`,
      }}
    >
      {glass ? (
        <>
          <div
            style={{
              position: "absolute",
              inset: 0,
              background: wallpaperBackground(-(glass.x + 9), -(glass.y + 9)),
            }}
          />
          <div
            style={{
              position: "absolute",
              inset: 0,
              background: `linear-gradient(to bottom, rgba(5,5,7,${GLASS.phoneAlpha}), rgba(17,17,20,${GLASS.phoneAlpha}))`,
            }}
          />
        </>
      ) : null}
      {/* status row */}
      <div
        style={{
          position: "absolute",
          top: 12,
          left: 26,
          right: 26,
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          zIndex: 5,
        }}
      >
        <span
          style={{
            fontSize: 14,
            fontWeight: 600,
            color: C.text,
            letterSpacing: 0.2,
            opacity: hideStatus ? 0 : 1,
          }}
        >
          9:41
        </span>
        <span
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            color: C.text,
          }}
        >
          {/* cellular bars (iOS status bar) */}
          <span style={{ display: "flex", alignItems: "flex-end", gap: 1.5, height: 11 }}>
            {[4.5, 6.5, 8.5, 11].map((h) => (
              <span
                key={h}
                style={{
                  width: 3,
                  height: h,
                  borderRadius: 1,
                  backgroundColor: C.text,
                }}
              />
            ))}
          </span>
          <Glyph size={14}>
            <path d="M3 9.5C8.2 4.6 15.8 4.6 21 9.5" />
            <path d="M6.2 13.2c3.4-3.1 8.2-3.1 11.6 0" />
            <circle cx={12} cy={18} r={1.3} fill="currentColor" stroke="none" />
          </Glyph>
          <span
            style={{
              width: 23,
              height: 12,
              borderRadius: 3.5,
              border: "1px solid rgba(255,255,255,0.5)",
              padding: 1.5,
              boxSizing: "border-box",
            }}
          >
            <span
              style={{
                display: "block",
                width: "72%",
                height: "100%",
                borderRadius: 1.5,
                backgroundColor: C.text,
              }}
            />
          </span>
        </span>
      </div>
      {/* dynamic island */}
      <div
        style={{
          position: "absolute",
          // the Dynamic Island: 125×37pt, 11pt down
          top: 8,
          left: (PHONE.screenW - 94) / 2,
          width: 94,
          height: 28,
          borderRadius: 999,
          backgroundColor: "#000000",
          border: "1px solid rgba(255,255,255,0.05)",
          zIndex: 5,
        }}
      />
      {children}
    </div>
  </div>
)

// ── The thread screen (ThreadScreen / AgentSessionView, N-12 + EXP-1334) ─────
// Every number in iOS points through `pt()`. The header: the back circle · the
// thread's REAL name (EXP-1341: a root started on an issue is
// "#<IDENT> <title>") · Stop (red text on the glass pill) while it runs, the
// `…` circle once it ended. Under it THE strip (ThreadStripTabs.swift): "+"
// and the icon-only Conversation tab pinned left, the issue chips (status
// glyph · mono identifier · title while it fits) between, the icon-only
// Guide tab (`<>`, green dot only while the Guide holds anything) pinned
// right; every chip 32pt tall at the 10pt row radius, the SHOWN page filled.

const reveal = (frame: number, at: number): React.CSSProperties => ({
  opacity: interpolate(frame, [at, at + 5], [0, 1], CLAMP),
  translate: `0px ${interpolate(frame, [at, at + 5], [8, 0], CLAMP)}px`,
})

export type ThreadPage = "conversation" | "issue" | "guide"

export const ThreadChrome: React.FC<{
  title: string
  running: boolean
  issue: { id: string; title: string; status: MobileStatus }
  shown?: ThreadPage
  guideDot?: boolean
  /** The issue tab's status glyph flips here (e.g. Done on merge). */
  issueStatusOverride?: MobileStatus
}> = ({ title, running, issue, shown = "conversation", guideDot, issueStatusOverride }) => {
  const NAV_Y = pt(79)
  const STRIP_Y = pt(130)
  const tabH = pt(32)
  const filled = (on: boolean): React.CSSProperties =>
    on
      ? {
          backgroundColor: IOS.fillActive,
          boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeActive}`,
        }
      : {
          backgroundColor: IOS.fillRow,
          boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
        }
  return (
    <>
      {/* header */}
      <div
        style={{
          position: "absolute",
          top: NAV_Y - pt(20.5),
          left: pt(19),
          right: pt(19),
          height: pt(41),
          display: "flex",
          alignItems: "center",
          gap: pt(12),
          zIndex: 2,
        }}
      >
        <NavCircle>
          <BackGlyph />
        </NavCircle>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: pt(17),
            fontWeight: 600,
            color: "#ffffff",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {title}
        </span>
        {running ? (
          <span
            style={{
              height: pt(41),
              boxSizing: "border-box",
              padding: `0 ${pt(14)}px`,
              borderRadius: 999,
              backgroundColor: IOS.opaqueCard,
              boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
              display: "flex",
              alignItems: "center",
              fontSize: pt(17),
              fontWeight: 500,
              color: IOS.red,
              flexShrink: 0,
            }}
          >
            Stop
          </span>
        ) : (
          <NavCircle>
            <MoreGlyph />
          </NavCircle>
        )}
      </div>

      {/* THE strip */}
      <div
        style={{
          position: "absolute",
          top: STRIP_Y - tabH / 2,
          left: pt(10),
          right: pt(10),
          height: tabH,
          display: "flex",
          alignItems: "center",
          gap: pt(6),
          zIndex: 2,
        }}
      >
        <span
          style={{
            width: pt(26),
            display: "flex",
            justifyContent: "center",
            color: IOS.secondary,
          }}
        >
          <Glyph size={pt(20)} sw={1.8}>
            <path d="M5 12h14" />
            <path d="M12 5v14" />
          </Glyph>
        </span>
        <span
          style={{
            width: pt(36),
            height: tabH,
            borderRadius: pt(10),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: shown === "conversation" ? "#ffffff" : IOS.secondary,
            ...(shown === "conversation"
              ? filled(true)
              : { backgroundColor: "transparent" }),
          }}
        >
          <Glyph size={pt(19)} sw={1.8}>
            <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
          </Glyph>
        </span>
        <span
          style={{
            minWidth: 0,
            flexShrink: 1,
            height: tabH,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: pt(6),
            padding: `0 ${pt(10)}px`,
            borderRadius: pt(10),
            ...filled(shown === "issue"),
          }}
        >
          <MStatusIcon status={issueStatusOverride ?? issue.status} size={pt(16)} />
          <span
            style={{
              fontFamily: MONO_FONT,
              fontSize: pt(14),
              color: IOS.secondary,
              flexShrink: 0,
            }}
          >
            {issue.id}
          </span>
          <span
            style={{
              fontSize: pt(15),
              color: IOS.secondary,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
              minWidth: 0,
            }}
          >
            {issue.title}
          </span>
        </span>
        <span style={{ flex: 1 }} />
        <span
          style={{
            position: "relative",
            width: pt(34),
            height: tabH,
            borderRadius: pt(10),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: shown === "guide" ? "#ffffff" : IOS.secondary,
            ...(shown === "guide" ? filled(true) : {}),
          }}
        >
          <Glyph size={pt(18)} sw={2}>
            <path d="m16 18 6-6-6-6" />
            <path d="m8 6-6 6 6 6" />
          </Glyph>
          {guideDot ? (
            <span
              style={{
                position: "absolute",
                top: pt(3),
                right: pt(3),
                width: pt(7),
                height: pt(7),
                borderRadius: 999,
                backgroundColor: IOS.green,
              }}
            />
          ) : null}
        </span>
      </div>
    </>
  )
}

// One Trace line (TraceView.swift `TraceLine`, Geometry.default): the concept
// glyph (16pt) · "{verb} {detail}" in 14pt on a 20pt line, muted.
const TRACE_VERBS: Record<string, { verb: string; icon: "read" | "edit" | "terminal" | "tool" }> = {
  Read: { verb: "Read", icon: "read" },
  Update: { verb: "Edited", icon: "edit" },
  Edit: { verb: "Edited", icon: "edit" },
  Write: { verb: "Edited", icon: "edit" },
  Bash: { verb: "Ran", icon: "terminal" },
}
const TraceGlyph: React.FC<{ icon: "read" | "edit" | "terminal" | "tool" }> = ({ icon }) => (
  <Glyph size={pt(16)} sw={1.8}>
    {icon === "read" ? (
      <>
        <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
        <path d="M14 2v4a2 2 0 0 0 2 2h4" />
        <path d="M10 9H8" />
        <path d="M16 13H8" />
        <path d="M16 17H8" />
      </>
    ) : icon === "edit" ? (
      <>
        <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />
        <path d="m15 5 4 4" />
      </>
    ) : icon === "terminal" ? (
      <>
        <polyline points="4 17 10 11 4 5" />
        <line x1="12" x2="20" y1="19" y2="19" />
      </>
    ) : (
      <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
    )}
  </Glyph>
)

const TraceRow: React.FC<{ frame: number; at: number; name: string; summary?: string }> = ({
  frame,
  at,
  name,
  summary,
}) => {
  if (frame < at) return null
  const v = TRACE_VERBS[name] ?? { verb: name, icon: "tool" as const }
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: pt(8),
        height: pt(20),
        color: IOS.muted,
        ...reveal(frame, at),
      }}
    >
      <TraceGlyph icon={v.icon} />
      <span
        style={{
          fontSize: pt(14),
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
          minWidth: 0,
        }}
      >
        {summary ? `${v.verb} ${summary}` : v.verb}
      </span>
    </div>
  )
}

// The agent's prose: the sparkle lead in the gutter, body text 14/22 white.
const ProseRow: React.FC<{ frame: number; at: number; text: string }> = ({ frame, at, text }) => {
  if (frame < at) return null
  return (
    <div style={{ display: "flex", gap: pt(8), ...reveal(frame, at) }}>
      <span style={{ color: IOS.tertiary, display: "flex", marginTop: pt(4), width: pt(16), justifyContent: "center" }}>
        <Glyph size={pt(13)} sw={1.8}>
          <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
        </Glyph>
      </span>
      <span style={{ flex: 1, fontSize: pt(14), lineHeight: `${pt(22)}px`, color: "#ffffff" }}>
        {text}
      </span>
    </div>
  )
}

// EXP-916: a run of edit calls is ONE card — "1 file edited" over the same
// DiffFileCard the Changes face draws, flush; the live row open.
const EditsCard: React.FC<{ frame: number; at: number }> = ({ frame, at }) => {
  if (frame < at) return null
  const file = CL_DIFF_FILES[0]
  return (
    <div
      style={{
        borderRadius: pt(10),
        overflow: "hidden",
        backgroundColor: IOS.fillRow,
        boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeRow}`,
        ...reveal(frame, at),
      }}
    >
      <div
        style={{
          padding: `${pt(8)}px ${pt(12)}px`,
          fontSize: pt(12),
          fontWeight: 500,
          color: IOS.secondary,
        }}
      >
        1 file edited
      </div>
      <DiffFileCard
        status={file.status}
        path={file.path}
        add={CL_FILE_STATS.add}
        del={CL_FILE_STATS.del}
        rows={CL_DIFF_ROWS.slice(0, 7)}
        compact
        carded={false}
        maxPathChars={30}
        rowOpacity={(i) => interpolate(frame, [at + 2 + i, at + 6 + i], [0, 1], CLAMP)}
      />
    </div>
  )
}

// The context ring (ContextRing.swift): a 16pt track with the used arc.
const ContextRing: React.FC<{ size: number; fraction: number }> = ({ size, fraction }) => {
  const r = 9
  const c = 2 * Math.PI * r
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={{ display: "block", flexShrink: 0 }}>
      <circle cx="12" cy="12" r={r} fill="none" stroke="rgba(255,255,255,0.18)" strokeWidth={3} />
      <circle
        cx="12"
        cy="12"
        r={r}
        fill="none"
        stroke="rgba(255,255,255,0.7)"
        strokeWidth={3}
        strokeLinecap="round"
        strokeDasharray={`${c * fraction} ${c}`}
        transform="rotate(-90 12 12)"
      />
    </svg>
  )
}

export type ThreadScreenProps = {
  frame: number
  feedSchedule: readonly number[] // per CL_PHONE_FEED item
  typeAt: number // the composer opens and the steer types (2 cpf)
  sendAt: number // send: the message lands in the transcript
  items?: readonly SteerItem[]
  message?: string
}

/** The live thread screen's CONTENT (no chassis) — shared with StartPhone,
 *  which pushes it in when the send opens the new thread. */
export const ThreadScreen: React.FC<ThreadScreenProps> = ({
  frame,
  feedSchedule,
  typeAt,
  sendAt,
  items = CL_PHONE_FEED,
  message = CL_STEER_MSG,
}) => {
  const blinkOn = useBlink(frame)
  const typing = frame >= typeAt && frame < sendAt
  const typedMsg = typing ? typed(message, frame, typeAt, 2) : ""
  const typingDone = frame >= typeAt + Math.ceil(message.length / 2)
  const sent = frame >= sendAt
  const open = frame >= typeAt
  const sendPop = sent ? spring({ frame: frame - sendAt, fps: 30, config: POP }) : 0
  const usage = 0.18 + 0.04 * interpolate(frame, [0, 260], [0, 1], CLAMP)

  return (
    <div style={{ position: "absolute", inset: 0, fontFamily: UI_FONT }}>
      <ThreadChrome
        title={`#${CL_ISSUE.id} ${CL_ISSUE.title}`}
        running
        issue={{ id: CL_ISSUE.id, title: CL_ISSUE.title, status: "in_progress" }}
        shown="conversation"
      />

      {/* the transcript: ONE scroll, bottom-anchored above the composer */}
      <div
        style={{
          position: "absolute",
          top: pt(152),
          left: pt(16),
          right: pt(16),
          bottom: open ? pt(186) : pt(104),
          display: "flex",
          flexDirection: "column",
          justifyContent: "flex-end",
          gap: pt(12),
          overflow: "hidden",
        }}
      >
        {items.map((item, i) => {
          const at = feedSchedule[i] ?? 0
          if (item.kind === "tool") {
            return TRACE_VERBS[item.name]?.icon === "edit" ? (
              <EditsCard key={i} frame={frame} at={at} />
            ) : (
              <TraceRow key={i} frame={frame} at={at} name={item.name} summary={item.summary} />
            )
          }
          return <ProseRow key={i} frame={frame} at={at} text={item.text} />
        })}
        {/* the sent steer: the caller's message, right-aligned, its byline under */}
        {sent ? (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "flex-end",
              gap: pt(4),
              scale: String(0.9 + 0.1 * sendPop),
              opacity: Math.min(1, sendPop * 2),
              transformOrigin: "bottom right",
            }}
          >
            <span
              style={{
                maxWidth: "86%",
                boxSizing: "border-box",
                fontSize: pt(14),
                lineHeight: `${pt(21)}px`,
                color: "#ffffff",
                backgroundColor: IOS.fillCard,
                boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
                borderRadius: pt(12),
                padding: `${pt(10)}px ${pt(12)}px`,
              }}
            >
              {message}
            </span>
            <span style={{ fontSize: pt(11), color: IOS.tertiary }}>
              {`${CL.user} · 9:41`}
            </span>
          </div>
        ) : null}
      </div>

      {/* THE composer (FloatingComposer): folded = the usage-ring circle +
          the "Type / for commands" capsule; open = the ONE card — field,
          then + · device pin · model pin · Plan · ring · send (N-12). An
          EMPTY field while the agent works offers Stop in the send slot. */}
      {!open ? (
        <div
          style={{
            position: "absolute",
            left: pt(20),
            right: pt(20),
            bottom: pt(4) + pt(34),
            display: "flex",
            alignItems: "center",
            gap: pt(10),
          }}
        >
          <BarCircle>
            <ContextRing size={pt(22)} fraction={usage} />
          </BarCircle>
          <span
            style={{
              flex: 1,
              height: pt(52),
              boxSizing: "border-box",
              borderRadius: 999,
              backgroundColor: IOS.opaqueCard,
              boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
              display: "flex",
              alignItems: "center",
              padding: `0 ${pt(18)}px`,
              fontSize: pt(17),
              color: IOS.tertiary,
            }}
          >
            Type / for commands
          </span>
        </div>
      ) : (
        <div
          style={{
            position: "absolute",
            left: pt(16),
            right: pt(16),
            bottom: pt(4) + pt(34),
            boxSizing: "border-box",
            borderRadius: pt(16),
            backgroundColor: IOS.opaqueCard,
            boxShadow: `inset 0 0 0 ${HAIRLINE}px ${typing ? "rgba(255,255,255,0.3)" : IOS.strokeCard}`,
          }}
        >
          <div
            style={{
              minHeight: pt(44),
              boxSizing: "border-box",
              padding: `${pt(12)}px ${pt(12)}px ${pt(6)}px`,
              fontSize: pt(17),
              lineHeight: `${pt(22)}px`,
              color: typedMsg ? "#ffffff" : IOS.tertiary,
            }}
          >
            {typedMsg || (typing ? "" : "Type / for commands")}
            {typing ? (
              <span
                style={{
                  display: "inline-block",
                  width: pt(2),
                  height: pt(19),
                  marginLeft: 1,
                  verticalAlign: `-${pt(3)}px`,
                  backgroundColor: "#ffffff",
                  opacity: !typingDone || blinkOn ? 1 : 0,
                }}
              />
            ) : null}
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: pt(2),
              padding: `0 ${pt(12)}px ${pt(10)}px`,
              color: IOS.secondary,
            }}
          >
            <span style={{ width: pt(36), height: pt(36), display: "flex", alignItems: "center", justifyContent: "center" }}>
              <Glyph size={pt(17)} sw={2}>
                <path d="M5 12h14" />
                <path d="M12 5v14" />
              </Glyph>
            </span>
            {/* the Device pin, icon-only on a phone (OptionPillLabel) */}
            <span
              style={{
                height: pt(28),
                padding: `0 ${pt(8)}px`,
                borderRadius: 999,
                backgroundColor: IOS.fillRow,
                boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
                display: "flex",
                alignItems: "center",
              }}
            >
              <Glyph size={pt(12)} sw={2}>
                <rect x="2" y="3" width="20" height="14" rx="2" />
                <path d="M8 21h8" />
                <path d="M12 17v4" />
              </Glyph>
            </span>
            <span style={{ display: "flex", alignItems: "center", gap: pt(4), padding: `0 ${pt(4)}px`, height: pt(36), fontSize: pt(15) }}>
              {PHONE_START_MODEL}
              <span style={{ color: IOS.tertiary, display: "flex" }}>
                <Glyph size={pt(10)} sw={2.4}>
                  <path d="m6 9 6 6 6-6" />
                </Glyph>
              </span>
            </span>
            <span style={{ display: "flex", alignItems: "center", gap: pt(6), padding: `0 ${pt(4)}px`, fontSize: pt(15) }}>
              Plan
              <span
                style={{
                  position: "relative",
                  width: pt(34),
                  height: pt(20),
                  borderRadius: 999,
                  backgroundColor: "rgba(120,120,128,0.32)",
                }}
              >
                <span
                  style={{
                    position: "absolute",
                    left: pt(2),
                    top: pt(2),
                    width: pt(16),
                    height: pt(16),
                    borderRadius: 999,
                    backgroundColor: "#ffffff",
                  }}
                />
              </span>
            </span>
            <span style={{ flex: 1 }} />
            <span style={{ width: pt(28), display: "flex", justifyContent: "center" }}>
              <ContextRing size={pt(16)} fraction={usage} />
            </span>
            <span
              style={{
                display: "flex",
                color: "#ffffff",
                marginLeft: pt(6),
                scale: sent ? String(1 + 0.1 * Math.max(0, 1 - (frame - sendAt) / 8)) : "1",
              }}
            >
              {sent ? (
                // empty field while the agent works: Stop in the send slot
                <Glyph size={pt(28)} sw={1.6}>
                  <circle cx="12" cy="12" r="10" />
                  <rect x="9" y="9" width="6" height="6" rx="1" />
                </Glyph>
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
      )}
    </div>
  )
}

// The model pin's label (the movie's run rides Claude Code on Opus, as the
// desktop session composer shows).
const PHONE_START_MODEL = "Opus"

// ── The session screen in its chassis ────────────────────────────────────────
export const SteerPhone: React.FC<
  ThreadScreenProps & { glass?: { x: number; y: number } }
> = ({ glass, ...props }) => (
  <PhoneChassis glass={glass}>
    <ThreadScreen {...props} />
  </PhoneChassis>
)
