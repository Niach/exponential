// closedloop/segments/startcoding.tsx — clip 1 (220f): pick an issue, hit
// Start coding, the agent goes to work in the dock. Opens FULLY COMPOSED —
// local frame 0 is the checked-in poster frame (bun run movie:poster).
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate, spring } from "remotion"
import { C, PAGE_FONT, SETTLE, WIN } from "../../ships/theme"
import {
  Camera,
  Caption,
  CursorLayer,
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
  CenterEmptyState,
  DockCollapsedStrip,
  IconRail,
  TabsBar,
  TopBar,
  type ChromeTab,
} from "../../ships/surfaces/chrome"
import { DETAIL_ANCHORS, IssueDetailPane } from "../../ships/surfaces/detail"
import { DialogScrim } from "../../ships/surfaces/dialogs"
import {
  START_DIALOG_ANCHORS,
  StartCodingDialog,
} from "../surfaces/startdialog"
import { TerminalDock, type DockTab } from "../../ships/surfaces/terminal"
import {
  CL,
  CL_BOARD,
  CL_ISSUE,
  CL_SESSION,
  COPY,
  NEW_ISSUE_ID,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP,
  CONTENT_TOP,
  RAIL_IDS,
  SegmentShell,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["start-coding"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  rowClick: 30,
  tabPop: 34,
  detailStagger: 40,
  startHover: 74,
  startHoverOut: 85,
  startClick: 86,
  dialogAppear: 94,
  checkPulse: 104,
  rowHover: { index: 1, at: 108, out: 120 },
  buttonHover: 122,
  starting: 128,
  collapse: 138,
  codingStart: 142, // EXP-151 todo → in_progress behind the dialog collapse
  dockAt: 146, // dock expands
  sessionTab: 152,
  feed: [156, 166, 178, 190, 200] as const, // first 5 CL_SESSION events
} as const

const CAPTIONS = {
  sc1: { in: 8, out: 88 },
  sc2: { in: 158, out: 208 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
// f0 is the poster: the WHOLE IDE window composed over the canvas
// (s 1.05 keeps all four window edges + margins inside the 1920×1080 frame).
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.05, x: 784, y: 490 },
  { f: 12, s: 1.05, x: 784, y: 490 },
  { f: 30, s: 1.9, x: 507, y: 331 },
  { f: 56, s: 1.9, x: 507, y: 331 },
  { f: 78, s: 1.75, x: 915, y: 325 },
  { f: 90, s: 1.75, x: 915, y: 325 },
  { f: 102, s: 1.7, x: 784, y: 500 },
  { f: 144, s: 1.7, x: 784, y: 500 },
  { f: 160, s: 1.7, x: 610, y: 718 },
]

// ── Cursor ────────────────────────────────────────────────────────────────────
const BOARD_ROW_151 = { x: 174, y: 206 } // sidebar: IP header+row, Todo header, EXP-151 first
const startCoding = {
  x: WIN.rail + WIN.sidebar + DETAIL_ANCHORS.startCoding.x,
  y: WIN.topBar + WIN.dockTabs + DETAIL_ANCHORS.startCoding.y,
}
const SD = START_DIALOG_ANCHORS

const CURSOR_KEYS: CursorKey[] = [
  { f: 0, x: 900, y: 560 },
  { f: 6, x: 900, y: 560 },
  { f: 22, x: BOARD_ROW_151.x, y: BOARD_ROW_151.y },
  { f: 44, x: BOARD_ROW_151.x, y: BOARD_ROW_151.y },
  { f: 64, x: startCoding.x, y: startCoding.y },
  { f: 92, x: startCoding.x, y: startCoding.y },
  { f: 104, x: SD.rows[1].row.x, y: SD.rows[1].row.y },
  { f: 112, x: SD.rows[1].row.x, y: SD.rows[1].row.y },
  { f: 118, x: SD.rows[3].row.x, y: SD.rows[3].row.y },
  { f: 122, x: SD.rows[3].row.x, y: SD.rows[3].row.y },
  { f: 126, x: SD.startCoding.x, y: SD.startCoding.y },
  { f: 134, x: SD.startCoding.x, y: SD.startCoding.y },
  { f: 146, x: 1240, y: 850 },
]
const CURSOR_CLICKS = [B.rowClick, B.startClick, B.starting]

const TAB_151: ChromeTab = { id: "exp151", label: NEW_ISSUE_ID, mono: true }
const DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  { id: "cl", label: CL.sessionTab, dot: C.green, popAt: B.sessionTab },
]

const dockHeightAt = (frame: number): number => {
  if (frame < B.dockAt) return WIN.dockStrip
  const t = spring({ frame: frame - B.dockAt, fps: 30, config: SETTLE })
  return WIN.dockStrip + (WIN.dockExpanded - WIN.dockStrip) * t
}

// ── The clip ──────────────────────────────────────────────────────────────────
export const StartCodingSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const dockH = dockHeightAt(frame)
  const paneH = WIN.h - CONTENT_TOP - dockH
  const captionSize = Math.round(72 * textScale)

  const heroStatus =
    frame >= B.codingStart ? ("in_progress" as const) : ("todo" as const)
  const overrides = { [NEW_ISSUE_ID]: { status: heroStatus } }
  const regroup =
    frame >= B.codingStart
      ? {
          id: NEW_ISSUE_ID,
          t: interpolate(
            frame,
            [B.codingStart, B.codingStart + 16],
            [0, 1],
            CLAMP
          ),
          from: "todo" as const,
        }
      : undefined

  return (
    <SegmentShell frame={frame} dur={DUR} openComposed>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TopBar
              frame={frame}
              projectName={CL.project}
              runConfig={CL.runConfig}
            />
            <IconRail frame={frame} active="issues" icons={RAIL_IDS} />
            <TabsBar
              frame={frame}
              tabs={[TAB_151]}
              activeId="exp151"
              popAt={{ exp151: B.tabPop }}
            />

            {/* sidebar board */}
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
                hover={{ id: NEW_ISSUE_ID, from: 20, to: B.rowClick }}
                selectedId={frame >= B.rowClick ? NEW_ISSUE_ID : undefined}
                regroup={regroup}
                showLabels={false}
              />
            </SidebarPane>

            {/* center: empty state until the issue opens */}
            {frame < B.tabPop + 8 ? (
              <div
                style={{
                  opacity: interpolate(
                    frame,
                    [B.tabPop, B.tabPop + 6],
                    [1, 0],
                    CLAMP
                  ),
                }}
              >
                <CenterEmptyState
                  frame={frame}
                  bottom={WIN.dockStrip}
                  contentCenter={{ x: 700, y: 380 }}
                />
              </div>
            ) : null}

            {/* center: issue detail */}
            {frame >= B.tabPop ? (
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
                  slideInAt={B.tabPop}
                  staggerAt={B.detailStagger}
                  startHover={{ at: B.startHover, out: B.startHoverOut }}
                  codingNow={{ at: B.sessionTab, out: DUR + 30 }}
                  status={heroStatus}
                  priority="none"
                  issue={CL_ISSUE}
                  width={CENTER_W}
                  height={paneH}
                />
              </div>
            ) : null}

            {/* dock */}
            {frame < B.dockAt ? (
              <DockCollapsedStrip frame={frame} count={1} />
            ) : (
              <TerminalDock
                frame={frame}
                height={dockH}
                tabs={DOCK_TABS}
                activeTab={frame < B.sessionTab ? "zsh" : "cl"}
                feed={{
                  events: CL_SESSION.slice(0, B.feed.length),
                  schedule: B.feed,
                }}
                spinnerBase={{ sec: 4, tokensK: 0.8 }}
              />
            )}

            {/* Start-coding dialog */}
            <DialogScrim frame={frame} in={B.dialogAppear} out={B.collapse} />
            {frame >= B.dialogAppear && frame < B.dockAt ? (
              <StartCodingDialog
                frame={frame}
                appearAt={B.dialogAppear}
                checkPulseAt={B.checkPulse}
                rowHover={B.rowHover}
                buttonState={{ hoverAt: B.buttonHover, startingAt: B.starting }}
                collapseAt={B.collapse}
              />
            ) : null}

            <CursorLayer
              keys={CURSOR_KEYS}
              clicks={CURSOR_CLICKS}
              frame={frame}
              from={0}
              to={160}
            />
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
