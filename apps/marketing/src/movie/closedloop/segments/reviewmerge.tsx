// closedloop/segments/reviewmerge.tsx — clip 3 (235f, extended by EXP-385):
// the PR's diff paints in the Changes tab, the rail switches to Reviews, the
// two-stage merge runs and the board regroups EXP-151 to Done — then, without
// ever leaving the app, the rail switches to Actions and the "Deploy
// storefront" runbook runs in the dock: merge → deploy. All beats are LOCAL
// frames.

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
  ActionsTool,
  BoardActions,
  BoardTool,
  ReviewsTool,
  SidebarPane,
  type MergeState,
} from "../../ships/surfaces/board"
import {
  DockCollapsedStrip,
  ExpandedRail,
  TitleBar,
  railRowCenter,
  type ChromeTab,
  type ExpandedRailProps,
} from "../../ships/surfaces/chrome"
import { IssueDetailPane } from "../../ships/surfaces/detail"
import { ChangesPane } from "../../ships/surfaces/diffview"
import { TerminalDock, type DockTab } from "../../ships/surfaces/terminal"
import {
  CL,
  CL_ACTIONS,
  CL_BOARD,
  CL_DEPLOY_SESSION,
  CL_DIFF_FILES,
  CL_DIFF_HEADER,
  CL_DIFF_ROWS,
  CL_FILE_STATS,
  CL_ISSUE,
  CL_REVIEW_ROW,
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

const DUR = SEGMENT_DURATIONS["review-merge"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  tabSwitch: 8,
  statsRoll: 12,
  paint: 16,
  fileSelect: 62,
  railTransition: 84,
  railClick: 88,
  sidebarSwapOut: 86, // board → reviews crossfade
  mergeHover: 100,
  confirmAt: 106, // click 1 → Confirm merge
  mergingAt: 116, // click 2 → Merging…
  rowFadeFrom: 124,
  rowFadeTo: 134,
  doneAt: 132, // EXP-151 regroups to Done
  regroupEnd: 150,
  sidebarSwapIn: 130, // reviews → board crossfade
  // The actions phase (EXP-385): merge → deploy, never leaving the app.
  actionsTransition: 154, // rail pill slides reviews → actions
  actionsClick: 156,
  actionsSwap: 158, // board → actions crossfade
  runHover: 168,
  runClick: 174, // Run → Running…
  deployDock: 178, // the dock springs open with the deploy session
  deployTab: 182,
  deployFeed: [186, 198, 210] as const, // CL_DEPLOY_SESSION events
} as const

const CAPTIONS = {
  rm1: { in: 10, out: 78 },
  rm2: { in: 108, out: 150 },
  rm3: { in: 182, out: 222 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.55, x: 940, y: 400 },
  { f: 14, s: 1.55, x: 940, y: 400 },
  { f: 80, s: 1.55, x: 940, y: 455, ease: "linear" },
  { f: 84, s: 1.55, x: 940, y: 455 },
  { f: 96, s: 1.7, x: 520, y: 360 },
  { f: 152, s: 1.7, x: 520, y: 360 },
  { f: 172, s: 1.25, x: 560, y: 530 },
]

// ── Cursor ────────────────────────────────────────────────────────────────────
const railReviews = railRowCenter("reviews")
const railActions = railRowCenter("actions")
const MERGE_BTN = { x: 641, y: 118 }
const CONFIRM_BTN = { x: 618, y: 118 }
const RUN_BTN = { x: 644, y: 118 } // first ActionsTool row's Run button

const CURSOR_KEYS: CursorKey[] = [
  { f: 74, x: 900, y: 400 },
  { f: 84, x: railReviews.x, y: railReviews.y },
  { f: 92, x: railReviews.x, y: railReviews.y },
  { f: 98, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 104, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 110, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 118, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 130, x: 700, y: 560 },
  { f: 144, x: 700, y: 560 },
  { f: 152, x: railActions.x, y: railActions.y },
  { f: 160, x: railActions.x, y: railActions.y },
  { f: 168, x: RUN_BTN.x, y: RUN_BTN.y },
  { f: 178, x: RUN_BTN.x, y: RUN_BTN.y },
  { f: 190, x: 900, y: 620 },
]
const CURSOR_CLICKS = [
  B.railClick,
  B.confirmAt,
  B.mergingAt,
  B.actionsClick,
  B.runClick,
]

const TAB_151: ChromeTab = {
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
}
const DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  {
    id: "deploy",
    label: CL_ACTIONS[0].name,
    dot: C.green,
    popAt: B.deployTab,
  },
]

const dockHeightAt = (frame: number): number => {
  if (frame < B.deployDock) return WIN.dockStrip
  const t = spring({ frame: frame - B.deployDock, fps: 30, config: SETTLE })
  return WIN.dockStrip + (WIN.dockExpanded - WIN.dockStrip) * t
}

// ── The clip ──────────────────────────────────────────────────────────────────
export const ReviewMergeSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const dockH = dockHeightAt(frame)
  const paneH = WIN.h - CONTENT_TOP - dockH
  const captionSize = Math.round(72 * textScale)

  const heroStatus =
    frame >= B.doneAt ? ("done" as const) : ("in_progress" as const)
  // Carried multiplayer state from the earlier clips.
  const overrides = {
    [NEW_ISSUE_ID]: { status: heroStatus },
    [REMOTE_DRAG_ID]: { status: "in_progress" as const },
    [LIVE_EDIT_ID]: { assignee: "JL" },
  }
  const regroup =
    frame >= B.doneAt
      ? {
          id: NEW_ISSUE_ID,
          t: interpolate(frame, [B.doneAt, B.regroupEnd], [0, 1], CLAMP),
          from: "in_progress" as const,
        }
      : undefined

  // Sidebar crossfades: board → reviews → board → actions.
  const fade6 = (at: number, from: number, to: number) =>
    interpolate(frame, [at, at + 6], [from, to], CLAMP)
  const boardO =
    frame < B.sidebarSwapIn
      ? fade6(B.sidebarSwapOut, 1, 0)
      : frame < B.actionsSwap
        ? fade6(B.sidebarSwapIn, 0, 1)
        : fade6(B.actionsSwap, 1, 0)
  const reviewsO = fade6(B.sidebarSwapOut, 0, 1) * fade6(B.sidebarSwapIn, 1, 0)
  const actionsO = fade6(B.actionsSwap, 0, 1)

  const mergeState: MergeState =
    frame < B.confirmAt
      ? "rest"
      : frame < B.mergingAt
        ? "confirm"
        : frame < B.rowFadeTo
          ? "merging"
          : "gone"
  const mergeMorphAt =
    mergeState === "confirm"
      ? B.confirmAt
      : mergeState === "merging"
        ? B.mergingAt
        : undefined
  const rowFade = interpolate(
    frame,
    [B.rowFadeFrom, B.rowFadeTo],
    [0, 1],
    CLAMP_EASE
  )

  const railProps: ExpandedRailProps =
    frame < B.railTransition
      ? { frame, active: "board" }
      : frame < B.actionsTransition
        ? {
            frame,
            active: "reviews",
            activeTransition: { from: "board", at: B.railTransition },
          }
        : {
            frame,
            active: "actions",
            activeTransition: { from: "reviews", at: B.actionsTransition },
          }
  const railDots = frame < B.railClick + 4 ? ["reviews"] : []

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TitleBar
              frame={frame}
              tabs={[
                { ...TAB_151, status: heroStatus },
              ]}
              activeId="exp151"
            />
            <ExpandedRail
              {...railProps}
              dots={railDots}
              boardName={CL.project}
              userName={CL.user}
              userInitial={CL.initials}
            />

            {/* sidebar: board */}
            {boardO > 0 &&
            (frame < B.sidebarSwapOut + 8 ||
              (frame >= B.sidebarSwapIn && frame < B.actionsSwap + 8)) ? (
              <div style={{ opacity: boardO }}>
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
                    prDotId={{ id: NEW_ISSUE_ID, at: 0 }}
                    regroup={regroup}
                    showLabels={false}
                  />
                </SidebarPane>
              </div>
            ) : null}

            {/* sidebar: reviews */}
            {frame >= B.railTransition && frame < B.sidebarSwapIn + 8 ? (
              <div style={{ opacity: reviewsO }}>
                <SidebarPane title="Reviews" bottomInset={dockH}>
                  <ReviewsTool
                    frame={frame}
                    mergeState={mergeState}
                    morphAt={mergeMorphAt}
                    hover={frame >= B.mergeHover && frame < B.confirmAt}
                    rowFade={rowFade}
                    row={CL_REVIEW_ROW}
                    project={CL.project}
                  />
                </SidebarPane>
              </div>
            ) : null}

            {/* sidebar: actions — merge flows straight into deploy */}
            {frame >= B.actionsSwap ? (
              <div style={{ opacity: actionsO }}>
                <SidebarPane title="Actions" bottomInset={dockH}>
                  <ActionsTool
                    frame={frame}
                    rows={CL_ACTIONS}
                    runId={CL_ACTIONS[0].id}
                    hoverAt={B.runHover}
                    runAt={B.runClick}
                    team={CL.project}
                  />
                </SidebarPane>
              </div>
            ) : null}

            {/* center: issue detail + Changes tab */}
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
                tab="changes"
                tabSwitchAt={B.tabSwitch}
                prChip={{ at: 0 }}
                status={heroStatus}
                priority="none"
                issue={CL_ISSUE}
                width={CENTER_W}
                height={paneH}
              />
              <div
                style={{
                  position: "absolute",
                  left: 0,
                  top: 34,
                  right: 0,
                  bottom: 0,
                  opacity: interpolate(
                    frame,
                    [B.tabSwitch, B.tabSwitch + 6],
                    [0, 1],
                    CLAMP
                  ),
                }}
              >
                <ChangesPane
                  frame={frame}
                  paintAt={B.paint}
                  statsRollAt={B.statsRoll}
                  fileSelectAt={B.fileSelect}
                  scrollY={0}
                  header={CL_DIFF_HEADER}
                  files={CL_DIFF_FILES}
                  rows={CL_DIFF_ROWS}
                  fileStats={CL_FILE_STATS}
                />
              </div>
            </div>

            {/* dock: collapsed until the deploy action spawns its session */}
            {frame < B.deployDock ? (
              <DockCollapsedStrip frame={frame} count={2} />
            ) : (
              <TerminalDock
                frame={frame}
                height={dockH}
                tabs={DOCK_TABS}
                activeTab={frame < B.deployTab ? "zsh" : "deploy"}
                feed={{
                  events: CL_DEPLOY_SESSION,
                  schedule: B.deployFeed,
                }}
                spinnerBase={{ sec: 2, tokensK: 0.3 }}
              />
            )}

            <CursorLayer
              keys={CURSOR_KEYS}
              clicks={CURSOR_CLICKS}
              frame={frame}
              from={74}
              to={196}
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
