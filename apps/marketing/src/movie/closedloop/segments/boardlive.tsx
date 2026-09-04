// closedloop/segments/boardlive.tsx — clip 1 (245f, the OPENER since
// EXP-385): multiplayer vibecoding. Opens FULLY COMPOSED — local frame 0 is
// the checked-in poster frame (bun run movie:poster): the whole IDE with
// EXP-151 open in Backlog and the phone beside it showing the SAME board in
// the real mobile app. EXP-149 moves Backlog → In Progress UNDER the local
// user — the way a teammate's change actually arrives: no presence facepile
// and no remote cursor (the product ships neither; there is no presence shape),
// just the row regrouping live AND the push banner dropping with the real
// notification copy ("Mara changed EXP-149 to In Progress"),
// a teammate's live edit flashes onto EXP-150. ONE static framing with a
// slow push — the camera never jumps (EXP-388). All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { PAGE_FONT, WIN } from "../../ships/theme"
import {
  Camera,
  Caption,
  CursorLayer,
  WindowChassis,
  shotKeys,
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
import { PhoneChassis } from "../surfaces/steerphone"
import { BoardScreen } from "../surfaces/mobileui"
import {
  CL,
  CL_BOARD,
  CL_ISSUE,
  CL_PHONE_BOARD,
  COPY,
  LIVE_EDIT_ID,
  NEW_ISSUE_ID,
  PUSH_NOTIFICATION,
  REMOTE_DRAG_ID,
} from "../fixtures"
import { OVERLAP, SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP_EASE,
  CONTENT_TOP,
  SegmentShell,
  captionSize,
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
  pushAt: 100, // the status change lands as a push banner (+ inbox rail dot)
  liveEdit: 160, // EXP-150: assignee pops in with a white flash
  remoteOut: 212,
} as const

const CAPTIONS = {
  bl1: { in: 8, out: 104 },
  bl2: { in: 118, out: 196 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
// ONE framing holds board + detail + phone for the whole clip; the only
// motion is a barely-there push (EXP-388: no big camera movements). f0 is
// the poster.
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.12, x: 850, y: 470 },
  { f: DUR, s: 1.15, x: 850, y: 470, ease: "linear" },
]

// Portrait (EXP-482, 1080×1350) can't hold both subjects either — the issue
// list ends at local x 684 and the phone starts at 1314 — so it stays two
// shots, but each subject now fits WHOLE: the full-height issue list column,
// then the entire phone. The cut sits in the caption gap (bl1 is gone by
// 110, bl2 arrives at 118) and 12f after the push lands, so the banner has
// fully sprung by the time we arrive.
// Shot A doubles as the PORTRAIT POSTER frame (movie:poster:portrait) —
// re-run that script if these numbers move.
const CAMERA_KEYS_PT: CamKey[] = shotKeys([
  { at: 0, s: 2.0, x: 424, y: 340 }, // the issue list column + facepile
  // x is biased left of the phone's own center so the visible band stays on
  // the set — the phone overhangs the window's right edge (1568), and past
  // comp 1920 there is nothing at all.
  { at: 112, s: 1.8, x: 1444, y: 569 }, // the WHOLE phone + push banner
])

// ── Cursors (window-local coords) ───────────────────────────────────────────
// The list pane starts under the 34px titlebar + the 44px Filter header, so
// row 0 sits at window y 78 and every row is 28 tall. Contract group order
// (backlog → in progress → done) puts, BEFORE the drag:
//   h:backlog 78 · EXP-151 106 · EXP-149 134 · EXP-150 162 · EXP-145 190 ·
//   EXP-146 218 · h:in-progress 246 · EXP-148 274 · h:done 302 · EXP-144 330 ·
//   EXP-147 358
// AFTER it, EXP-149 lands at 274 (under EXP-148) and EXP-150 rises to 134;
// everything from h:in-progress down is unmoved. Cursor Ys are row centers
// (top + 14).
const LOCAL_KEYS: CursorKey[] = [
  { f: 0, x: 900, y: 420 },
  { f: 52, x: 900, y: 420 },
  { f: 80, x: 345, y: 148 }, // EXP-150, where the regroup leaves it
  { f: 150, x: 345, y: 148 },
  { f: 168, x: 345, y: 148 },
  { f: 190, x: 900, y: 500 },
]

const TAB_151: ChromeTab = {
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
  status: "backlog",
}

// Phone placement in COMP coordinates inside the camera layer (floats over
// the window's right edge; unscaled so the whole device fits the framing).
const PHONE_POS = { x: 1490, y: 280, scale: 1 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const BoardLiveSegment: React.FC<SegmentProps> = ({
  frame,
  portrait,
}) => {
  const dockH = WIN.dockStrip
  const paneH = WIN.h - CONTENT_TOP - dockH
  const capSize = captionSize(portrait)

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
        from: "backlog" as const,
      }
    : undefined

  // The mobile list regroups WITH Mara's drag (slightly behind the cursor).
  const phoneMoveT = interpolate(
    frame,
    [B.dragTo - 6, B.dragTo + 10],
    [0, 1],
    CLAMP_EASE
  )

  return (
    <SegmentShell frame={frame} dur={DUR} openComposed>
      <AbsoluteFill>
        <Camera keys={portrait ? CAMERA_KEYS_PT : CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TitleBar frame={frame} tabs={[TAB_151]} activeId="exp151" />
            <ExpandedRail
              frame={frame}
              active="board"
              dots={frame >= B.pushAt + 2 ? ["inbox"] : []}
              boardName={CL.project}
              userName={CL.user}
              userInitial={CL.initials}
            />

            <SidebarPane actions={<BoardActions />} bottomInset={dockH}>
              <BoardTool
                frame={frame}
                rows={CL_BOARD}
                overrides={overrides}
                hover={{ id: LIVE_EDIT_ID, from: 84, to: 150 }}
                selectedId={NEW_ISSUE_ID}
                regroup={regroup}
                flashAt={{ id: LIVE_EDIT_ID, at: B.liveEdit }}
              />
            </SidebarPane>

            {/* center: EXP-151 open in Backlog — the state feedback wraps into */}
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
                status="backlog"
                priority="none"
                issue={CL_ISSUE}
                width={CENTER_W}
                height={paneH}
              />
            </div>

            <DockCollapsedStrip frame={frame} count={1} />

            {/* to reaches into the cross-fade overrun (EXP-482) so the
                cursor doesn't pop off while the clip is still opaque */}
            <CursorLayer
              keys={LOCAL_KEYS}
              frame={frame}
              from={0}
              to={DUR + OVERLAP}
            />
          </WindowChassis>

          {/* the phone, composed from f0 (it IS part of the poster): the real
              mobile board list — Mara's drag regroups it live, then the push
              banner drops with the real notification copy */}
          <div
            style={{
              position: "absolute",
              left: PHONE_POS.x,
              top: PHONE_POS.y,
            }}
          >
            <div
              style={{
                transform: `scale(${PHONE_POS.scale})`,
                transformOrigin: "0 0",
              }}
            >
              <PhoneChassis glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}>
                <BoardScreen
                  frame={frame}
                  boardName={CL.project}
                  rows={CL_PHONE_BOARD}
                  overrides={{ [REMOTE_DRAG_ID]: "in_progress" }}
                  moveT={phoneMoveT}
                  tabDots={frame >= B.pushAt + 2 ? ["mywork"] : []}
                  banner={{
                    at: B.pushAt,
                    title: PUSH_NOTIFICATION.title,
                    body: PUSH_NOTIFICATION.body,
                  }}
                />
              </PhoneChassis>
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
          size={capSize}
          centered={portrait}
          fontFamily={PAGE_FONT}
          letterSpacing="-0.03em"
        >
          {COPY[key]}
        </Caption>
      ))}
    </SegmentShell>
  )
}
