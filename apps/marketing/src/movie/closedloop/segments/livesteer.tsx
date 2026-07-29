// closedloop/segments/livesteer.tsx — clip 2 (210f, NEW in EXP-337): the
// session streams in the dock, the camera pulls wide to an iPhone floating
// beside the window showing the mobile steer activity view, the user types a
// steer on the phone and sends it — it lands highlighted in the terminal
// (TerminalDock's inputGlow beat) and the agent acknowledges and continues.
// No local desktop cursor in this clip: the hands are on the phone.
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { C, PAGE_FONT, WIN } from "../../ships/theme"
import { Camera, Caption, WindowChassis, type CamKey } from "../../ships/rig"
import {
  BoardActions,
  BoardTool,
  SidebarPane,
} from "../../ships/surfaces/board"
import {
  IconRail,
  TabsBar,
  TopBar,
  type ChromeTab,
} from "../../ships/surfaces/chrome"
import { IssueDetailPane } from "../../ships/surfaces/detail"
import { TerminalDock, type DockTab } from "../../ships/surfaces/terminal"
import type { SessionEvent } from "../../ships/fixtures"
import { SteerPhone } from "../surfaces/steerphone"
import {
  CL,
  CL_BOARD,
  CL_ISSUE,
  CL_STEER_MSG,
  CL_STEER_REPLY,
  CL_STEER_SESSION,
  COPY,
  NEW_ISSUE_ID,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP_EASE,
  CONTENT_TOP,
  RAIL_IDS,
  SegmentShell,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["live-steer"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  phoneIn: 56, // the iPhone rises in beside the window
  phoneFeed: [60, 66, 74] as const, // CL_PHONE_FEED mirror rows
  typeAt: 84, // steer typing on the phone (2 cpf)
  sendAt: 130, // send tap → user bubble
  steerGlow: 136, // prompt-box indigo pulse in the dock
  steerLand: 140, // the steer lands as a highlighted terminal line
  reply: [150, 162, 178] as const, // CL_STEER_REPLY events
} as const

const CAPTIONS = { ls1: { in: 66, out: 128 } } as const

// The dock feed: the running session, then the landed steer, then the reply.
const STEER_EVENT: SessionEvent = {
  kind: "flash",
  text: `Steer from phone: ${CL_STEER_MSG}`,
}
const FEED_EVENTS: SessionEvent[] = [
  ...CL_STEER_SESSION,
  STEER_EVENT,
  ...CL_STEER_REPLY,
]
const FEED_SCHEDULE = [8, 16, 28, 40, B.steerLand, ...B.reply]

// ── Camera ────────────────────────────────────────────────────────────────────
// Shot A: tight on the streaming dock. Shot B: pull wide — phone + dock share
// the frame while the steer is typed. Shot C: back onto the dock for the
// landing + reply (the phone exits frame-right naturally).
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.7, x: 610, y: 718 },
  { f: 46, s: 1.7, x: 610, y: 718 },
  { f: 66, s: 1.32, x: 1010, y: 640 },
  { f: 138, s: 1.32, x: 1010, y: 640 },
  { f: 154, s: 1.6, x: 680, y: 700 },
]

const TAB_151: ChromeTab = { id: "exp151", label: NEW_ISSUE_ID, mono: true }
const DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  { id: "cl", label: CL.sessionTab, dot: C.green },
]

// Phone placement in COMP coordinates inside the camera layer (floats over
// the window's right edge; scaled for legibility at the Shot B zoom).
const PHONE_POS = { x: 1490, y: 300, scale: 1.15 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const LiveSteerSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const dockH = WIN.dockExpanded
  const paneH = WIN.h - CONTENT_TOP - dockH
  const captionSize = Math.round(72 * textScale)

  const phoneRise = interpolate(
    frame,
    [B.phoneIn, B.phoneIn + 14],
    [0, 1],
    CLAMP_EASE
  )

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TopBar
              frame={frame}
              projectName={CL.project}
              runConfig={CL.runConfig}
            />
            <IconRail frame={frame} active="issues" icons={RAIL_IDS} />
            <TabsBar frame={frame} tabs={[TAB_151]} activeId="exp151" />

            <SidebarPane
              title="All Issues"
              actions={<BoardActions />}
              pills
              bottomInset={dockH}
            >
              <BoardTool
                frame={frame}
                rows={CL_BOARD}
                overrides={{ [NEW_ISSUE_ID]: { status: "in_progress" } }}
                selectedId={NEW_ISSUE_ID}
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
                codingNow={{ at: 0, out: DUR + 30 }}
                status="in_progress"
                priority="none"
                issue={CL_ISSUE}
                width={CENTER_W}
                height={paneH}
              />
            </div>

            <TerminalDock
              frame={frame}
              height={dockH}
              tabs={DOCK_TABS}
              activeTab="cl"
              feed={{ events: FEED_EVENTS, schedule: FEED_SCHEDULE }}
              inputGlow={B.steerGlow}
              spinnerBase={{ sec: 74, tokensK: 5.2 }}
            />
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
              <SteerPhone
                frame={frame}
                feedSchedule={B.phoneFeed}
                typeAt={B.typeAt}
                sendAt={B.sendAt}
              />
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
