// closedloop/segments/codeeverywhere.tsx — clip 2 (260f, EXP-385: the merged
// start-coding + live-steer clip). Code from everywhere, running locally:
// the phone rises with EXP-151's mobile issue view, the start sheet slides up
// (agent tabs · online device · model — a long dwell, this IS the film's
// start-coding dialog) and on Start the desktop reacts SIMULTANEOUSLY — the
// dock springs open with the session tab, EXP-151 FLIPs Todo → In Progress
// and the "Coding now" pill pops. The phone flips to the steer activity view,
// mirrors the feed, and a typed steer lands highlighted in the terminal.
// No local desktop cursor in this clip: the hands are on the phone.
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate, spring } from "remotion"
import { C, PAGE_FONT, SETTLE, WIN } from "../../ships/theme"
import { Camera, Caption, WindowChassis, type CamKey } from "../../ships/rig"
import {
  BoardActions,
  BoardTool,
  SidebarPane,
} from "../../ships/surfaces/board"
import {
  DockCollapsedStrip,
  ExpandedRail,
  TitleBar,
  type ChromeTab,
} from "../../ships/surfaces/chrome"
import { IssueDetailPane } from "../../ships/surfaces/detail"
import { TerminalDock, type DockTab } from "../../ships/surfaces/terminal"
import type { SessionEvent } from "../../ships/fixtures"
import { StartPhone } from "../surfaces/startphone"
import { SteerPhone } from "../surfaces/steerphone"
import {
  CL,
  CL_BOARD,
  CL_ISSUE,
  CL_SESSION,
  CL_STEER_MSG,
  CL_STEER_REPLY,
  COPY,
  LIVE_EDIT_ID,
  NEW_ISSUE_ID,
  REMOTE_DRAG_ID,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP,
  CLAMP_EASE,
  CONTENT_TOP,
  SegmentShell,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["code-everywhere"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  phoneIn: 8, // the iPhone rises with the EXP-151 issue view
  tapAt: 28, // Start-coding button tap
  sheetAt: 34, // the start sheet slides up — then DWELLS (~70f)
  flick: { at: 58, out: 66 }, // hover flick across the Codex tab
  startAt: 104, // sheet Start press → "Starting…"
  simul: 112, // the desktop reacts: dock springs, tab pops, board FLIPs
  sessionTab: 118,
  sheetOut: 118, // sheet collapses; the phone flips to the steer view
  phoneSwap: 122,
  feed: [124, 134, 146, 158, 168] as const, // first 5 CL_SESSION events
  phoneFeed: [128, 140, 152] as const, // CL_PHONE_FEED mirror rows
  typeAt: 168, // steer typing on the phone (2 cpf)
  sendAt: 210, // send tap → user bubble
  steerGlow: 214, // prompt-box indigo pulse in the dock
  steerLand: 218, // the steer lands as a highlighted terminal line
  reply: [226, 238, 250] as const, // CL_STEER_REPLY events
} as const

const CAPTIONS = {
  ce1: { in: 20, out: 100 },
  ce2: { in: 116, out: 162 },
  ce3: { in: 214, out: 250 },
} as const

// The dock feed: the spawned session, then the landed steer, then the reply.
const STEER_EVENT: SessionEvent = {
  kind: "flash",
  text: `Steer from phone: ${CL_STEER_MSG}`,
}
const FEED_EVENTS: SessionEvent[] = [
  ...CL_SESSION.slice(0, B.feed.length),
  STEER_EVENT,
  ...CL_STEER_REPLY,
]
const FEED_SCHEDULE = [...B.feed, B.steerLand, ...B.reply]

// ── Camera ────────────────────────────────────────────────────────────────────
// One wide two-hander holds phone + board + dock through the start and the
// simultaneity beat, then eases onto the dock for the steer landing (the
// phone stays half in frame, still live).
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.3, x: 1010, y: 620 },
  { f: 208, s: 1.3, x: 1010, y: 620 },
  { f: 238, s: 1.5, x: 780, y: 700 },
]

const TAB_151 = (frame: number): ChromeTab => ({
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
  status: frame >= B.simul ? "in_progress" : "todo",
})
const DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  { id: "cl", label: CL.sessionTab, dot: C.green, popAt: B.sessionTab },
]

const dockHeightAt = (frame: number): number => {
  if (frame < B.simul) return WIN.dockStrip
  const t = spring({ frame: frame - B.simul, fps: 30, config: SETTLE })
  return WIN.dockStrip + (WIN.dockExpanded - WIN.dockStrip) * t
}

// Phone placement in COMP coordinates inside the camera layer.
const PHONE_POS = { x: 1490, y: 300, scale: 1.15 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const CodeEverywhereSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const dockH = dockHeightAt(frame)
  const paneH = WIN.h - CONTENT_TOP - dockH
  const captionSize = Math.round(72 * textScale)

  const heroStatus =
    frame >= B.simul ? ("in_progress" as const) : ("todo" as const)
  // Carried multiplayer state from clip 1: Mara's drag + the live edit.
  const overrides = {
    [NEW_ISSUE_ID]: { status: heroStatus },
    [REMOTE_DRAG_ID]: { status: "in_progress" as const },
    [LIVE_EDIT_ID]: { assignee: "JL" },
  }
  const regroup =
    frame >= B.simul
      ? {
          id: NEW_ISSUE_ID,
          t: interpolate(frame, [B.simul, B.simul + 16], [0, 1], CLAMP),
          from: "todo" as const,
        }
      : undefined

  const phoneRise = interpolate(
    frame,
    [B.phoneIn, B.phoneIn + 14],
    [0, 1],
    CLAMP_EASE
  )
  // Start view → steer view crossfade while the sheet collapses.
  const startO = interpolate(
    frame,
    [B.phoneSwap, B.phoneSwap + 8],
    [1, 0],
    CLAMP
  )

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TitleBar frame={frame} tabs={[TAB_151(frame)]} activeId="exp151" />
            <ExpandedRail
              frame={frame}
              active="board"
              boardName={CL.project}
              userName={CL.user}
              userInitial={CL.initials}
            />

            <SidebarPane
              title="All Issues"
              actions={<BoardActions />}
              pills
              bottomInset={dockH}
            >
              <BoardTool
                frame={frame}
                rows={CL_BOARD}
                overrides={overrides}
                selectedId={NEW_ISSUE_ID}
                regroup={regroup}
                showLabels={false}
              />
            </SidebarPane>

            <div
              style={{
                position: "absolute",
                left: CENTER_X,
                top: CONTENT_TOP,
                width: CENTER_W,
                height: paneH,
                overflow: "hidden",
              }}
            >
              <IssueDetailPane
                frame={frame}
                tab="details"
                codingNow={{ at: B.sessionTab, out: DUR + 30 }}
                status={heroStatus}
                priority="none"
                issue={CL_ISSUE}
                width={CENTER_W}
                height={paneH}
              />
            </div>

            {frame < B.simul ? (
              <DockCollapsedStrip frame={frame} count={1} />
            ) : (
              <TerminalDock
                frame={frame}
                height={dockH}
                tabs={DOCK_TABS}
                activeTab={frame < B.sessionTab ? "zsh" : "cl"}
                feed={{ events: FEED_EVENTS, schedule: FEED_SCHEDULE }}
                inputGlow={B.steerGlow}
                spinnerBase={{ sec: 4, tokensK: 0.8 }}
              />
            )}
          </WindowChassis>

          {/* the phone, floating over the window's right edge (comp coords) */}
          <div
            style={{
              position: "absolute",
              left: PHONE_POS.x,
              top: PHONE_POS.y,
              opacity: phoneRise,
              translate: `0px ${(1 - phoneRise) * 46}px`,
            }}
          >
            <div
              style={{
                transform: `scale(${PHONE_POS.scale})`,
                transformOrigin: "0 0",
              }}
            >
              {startO > 0 ? (
                <div style={{ opacity: startO }}>
                  <StartPhone
                    frame={frame}
                    tapAt={B.tapAt}
                    sheetAt={B.sheetAt}
                    flickAt={B.flick}
                    startAt={B.startAt}
                    collapseAt={B.sheetOut}
                    glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}
                  />
                </div>
              ) : null}
              {frame >= B.phoneSwap ? (
                <div
                  style={{
                    position: "absolute",
                    left: 0,
                    top: 0,
                    opacity: 1 - startO,
                  }}
                >
                  <SteerPhone
                    frame={frame}
                    feedSchedule={B.phoneFeed}
                    typeAt={B.typeAt}
                    sendAt={B.sendAt}
                    glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}
                  />
                </div>
              ) : null}
            </div>
          </div>
        </Camera>
      </AbsoluteFill>

      {(Object.keys(CAPTIONS) as (keyof typeof CAPTIONS)[]).map((key) => (
        <Caption
          key={key}
          frame={frame}
          in={CAPTIONS[key].in}
          out={CAPTIONS[key].out}
          size={captionSize}
          fontFamily={PAGE_FONT}
          letterSpacing="-0.03em"
        >
          {COPY[key]}
        </Caption>
      ))}
    </SegmentShell>
  )
}
