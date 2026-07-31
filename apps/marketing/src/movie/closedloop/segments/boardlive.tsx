// closedloop/segments/boardlive.tsx — clip 1 (245f, the OPENER since
// EXP-385): multiplayer vibecoding. Opens FULLY COMPOSED — local frame 0 is
// the checked-in poster frame (bun run movie:poster): the whole IDE with
// EXP-151 open in Todo. A presence facepile pops into the titlebar, Mara's
// colored remote cursor drags EXP-149 Todo → In Progress (the FLIP regroup
// reads as the drag), the phone rises and the status change lands on it as a
// PUSH notification (the inbox rail dot pops with it), a teammate's live
// edit flashes onto EXP-150, and the local cursor moves simultaneously.
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { PAGE_FONT, WIN } from "../../ships/theme"
import {
  Camera,
  Caption,
  CursorLayer,
  RemoteCursor,
  WindowChassis,
  type CamKey,
  type CursorKey,
} from "../../ships/rig"
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
import { NotifPhone } from "../surfaces/steerphone"
import {
  CL,
  CL_BOARD,
  CL_ISSUE,
  COPY,
  LIVE_EDIT_ID,
  NEW_ISSUE_ID,
  PRESENCE_USERS,
  REMOTE_DRAG_ID,
  REMOTE_USER,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP_EASE,
  CONTENT_TOP,
  SegmentShell,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["board-live"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  presenceAt: 10,
  remoteIn: 30,
  dragPress: 56,
  dragFrom: 58,
  dragTo: 78,
  phoneIn: 88, // the iPhone rises beside the window
  pushAt: 112, // the status change lands as a push banner (+ inbox rail dot)
  liveEdit: 160, // EXP-150: assignee pops in with an indigo flash
  phoneOut: 190,
  remoteOut: 212,
} as const

const CAPTIONS = {
  bl1: { in: 8, out: 104 },
  bl2: { in: 118, out: 196 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
// f0 is the poster: the WHOLE IDE window composed over the canvas. Then push
// onto the board for the drag (facepile bound: y ≤ 540/s keeps y0 visible),
// widen to the two-hander for the phone push, and settle back.
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.05, x: 784, y: 490 },
  { f: 14, s: 1.05, x: 784, y: 490 },
  { f: 32, s: 1.7, x: 440, y: 270 },
  { f: 84, s: 1.7, x: 440, y: 270 },
  { f: 100, s: 1.32, x: 1010, y: 500 },
  { f: 188, s: 1.32, x: 1010, y: 500 },
  { f: 206, s: 1.55, x: 600, y: 330 },
]

// ── Cursors (window-local tool-window coords; rows y = 104 + layout offset) ──
// Before the drag: h:ip 104, EXP-148 132, h:todo 160, EXP-151 188, EXP-149 216,
// EXP-150 244. After: EXP-149 lands at 160 (after EXP-148 inside In Progress).
const REMOTE_KEYS: CursorKey[] = [
  { f: B.remoteIn, x: 220, y: 146 },
  { f: 44, x: 360, y: 230 },
  { f: B.dragFrom, x: 360, y: 230 },
  { f: B.dragTo, x: 360, y: 174 },
  { f: 92, x: 360, y: 174 },
  { f: 118, x: 420, y: 320 },
  { f: 150, x: 420, y: 320 },
  { f: B.remoteOut, x: 110, y: 260 },
]

const LOCAL_KEYS: CursorKey[] = [
  { f: 0, x: 900, y: 420 },
  { f: 52, x: 900, y: 420 },
  { f: 70, x: 340, y: 258 }, // EXP-150 (its slot is stable through the regroup)
  { f: 150, x: 340, y: 258 },
  { f: 168, x: 340, y: 258 },
  { f: 190, x: 900, y: 500 },
]

const TAB_151: ChromeTab = {
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
  status: "todo",
}

// Phone placement in COMP coordinates inside the camera layer (floats over
// the window's right edge; scaled for legibility at the two-hander zoom).
const PHONE_POS = { x: 1490, y: 300, scale: 1.15 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const BoardLiveSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const dockH = WIN.dockStrip
  const paneH = WIN.h - CONTENT_TOP - dockH
  const captionSize = Math.round(72 * textScale)

  const dragging = frame >= B.dragFrom
  const overrides: Record<
    string,
    { status?: "in_progress"; assignee?: string }
  > = {
    ...(dragging
      ? { [REMOTE_DRAG_ID]: { status: "in_progress" as const } }
      : {}),
    ...(frame >= B.liveEdit ? { [LIVE_EDIT_ID]: { assignee: "JL" } } : {}),
  }
  const regroup = dragging
    ? {
        id: REMOTE_DRAG_ID,
        t: interpolate(frame, [B.dragFrom, B.dragTo], [0, 1], CLAMP_EASE),
        from: "todo" as const,
      }
    : undefined

  const phoneRise =
    interpolate(frame, [B.phoneIn, B.phoneIn + 14], [0, 1], CLAMP_EASE) *
    interpolate(frame, [B.phoneOut, B.phoneOut + 12], [1, 0], CLAMP_EASE)

  return (
    <SegmentShell frame={frame} dur={DUR} openComposed>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TitleBar
              frame={frame}
              tabs={[TAB_151]}
              activeId="exp151"
              presence={{ users: PRESENCE_USERS, at: B.presenceAt }}
            />
            <ExpandedRail
              frame={frame}
              active="board"
              dots={frame >= B.pushAt + 2 ? ["inbox"] : []}
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
                hover={{ id: LIVE_EDIT_ID, from: 76, to: 150 }}
                selectedId={NEW_ISSUE_ID}
                regroup={regroup}
                flashAt={{ id: LIVE_EDIT_ID, at: B.liveEdit }}
                showLabels={false}
              />
            </SidebarPane>

            {/* center: EXP-151 open in Todo — the state feedback wraps into */}
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
                status="todo"
                priority="none"
                issue={CL_ISSUE}
                width={CENTER_W}
                height={paneH}
              />
            </div>

            <DockCollapsedStrip frame={frame} count={1} />

            {/* Mara's remote cursor drags the row; the local cursor moves at
                the same time — simultaneity is the story. */}
            <RemoteCursor
              keys={REMOTE_KEYS}
              clicks={[B.dragPress]}
              frame={frame}
              from={B.remoteIn}
              to={B.remoteOut}
              name={REMOTE_USER.name}
              color={REMOTE_USER.color}
            />
            <CursorLayer keys={LOCAL_KEYS} frame={frame} from={0} to={DUR} />
          </WindowChassis>

          {/* the phone, floating over the window's right edge (comp coords):
              Mara's drag arrives as a push on the lock screen */}
          {phoneRise > 0.01 ? (
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
                <NotifPhone
                  frame={frame}
                  bannerAt={B.pushAt}
                  glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}
                />
              </div>
            </div>
          ) : null}
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
