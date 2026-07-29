// closedloop/segments/boardlive.tsx — clip 4 (195f, NEW in EXP-337):
// realtime board multiplayer. A presence facepile pops into the top bar,
// Mara's colored remote cursor drags EXP-149 Todo → In Progress (the FLIP
// regroup reads as the drag), a teammate's live edit flashes onto EXP-150
// (an assignee appears), and the local cursor moves simultaneously.
// EXP-151 sits in Done — the post-merge state. All beats are LOCAL frames.

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
  presenceAt: 12,
  remoteIn: 26,
  dragPress: 54,
  dragFrom: 56,
  dragTo: 76,
  remoteOut: 150,
  liveEdit: 108, // EXP-150: assignee pops in with an indigo flash
} as const

const CAPTIONS = { bl1: { in: 16, out: 170 } } as const

// ── Camera ────────────────────────────────────────────────────────────────────
// Framed high enough that the titlebar's presence facepile stays in shot
// (y ≤ 540/s keeps window-local y0 visible).
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.55, x: 600, y: 330 },
  { f: 44, s: 1.55, x: 600, y: 330 },
  { f: 58, s: 1.7, x: 440, y: 270 },
  { f: 92, s: 1.7, x: 440, y: 270 },
  { f: 108, s: 1.55, x: 600, y: 330 },
]

// ── Cursors (window-local tool-window coords; rows y = 104 + layout offset) ──
// Before the drag: h:ip 104, EXP-148 132, h:todo 160, EXP-149 188, EXP-150 216.
// After: EXP-149 lands at 160 (after EXP-148 inside In Progress).
const REMOTE_KEYS: CursorKey[] = [
  { f: B.remoteIn, x: 220, y: 146 },
  { f: 40, x: 360, y: 202 },
  { f: B.dragFrom, x: 360, y: 202 },
  { f: B.dragTo, x: 360, y: 174 },
  { f: 88, x: 360, y: 174 },
  { f: 104, x: 420, y: 300 },
  { f: 132, x: 420, y: 300 },
  { f: B.remoteOut, x: 110, y: 260 },
]

const LOCAL_KEYS: CursorKey[] = [
  { f: 0, x: 900, y: 420 },
  { f: 50, x: 900, y: 420 },
  { f: 66, x: 340, y: 230 + 28 }, // EXP-150 (shifted down while EXP-149 is mid-flight… settles)
  { f: 70, x: 340, y: 230 },
  { f: 100, x: 340, y: 230 },
  { f: 118, x: 900, y: 500 },
]

const TAB_151: ChromeTab = {
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
  status: "done",
}

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
    { status?: "done" | "in_progress"; assignee?: string }
  > = {
    [NEW_ISSUE_ID]: { status: "done" },
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

  return (
    <SegmentShell frame={frame} dur={DUR}>
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
                hover={{ id: LIVE_EDIT_ID, from: 70, to: 100 }}
                regroup={regroup}
                flashAt={{ id: LIVE_EDIT_ID, at: B.liveEdit }}
                prDotId={{ id: NEW_ISSUE_ID, at: 0 }}
                showLabels={false}
              />
            </SidebarPane>

            {/* center: the merged issue's detail, at rest */}
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
                prChip={{ at: 0 }}
                status="done"
                priority="none"
                issue={CL_ISSUE}
                width={CENTER_W}
                height={paneH}
              />
            </div>

            <DockCollapsedStrip frame={frame} count={2} />

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
