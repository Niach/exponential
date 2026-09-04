// closedloop/segments/reviewmerge.tsx — clip 3 (235f, refocused by EXP-388):
// ONE surface, one framing. The PR's diff paints into the PrDiff center
// screen (the screen Reviews rows open — the old Changes tab is gone) beside
// the board's issue list, exactly as shots/review-diff/desktop.webp frames it;
// the rail's Reviews entry carries its green open-PR dot. EXP-706 retired the
// docked Reviews TOOL window (Reviews is a tab-less full-page screen now), so
// the two-stage merge runs where the product puts it: the diff header's own
// merge cluster (Merge → Confirm merge → Merging…). The phone beside the window shows the
// REAL mobile issue detail: its coding/PR card flips "Ready for review" →
// "Merged" and the status chip lands on Done as the merge completes.
// PORTRAIT (FEED-20): the phone IS the clip — it shows the mobile Review
// screen instead (diff cards + the × · Merge · ↗ bottom bar) and the merge
// runs THERE: tap Merge → "Merge pull request?" → spinner → Merged.
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { C, PAGE_FONT, WIN } from "../../ships/theme"
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
import {
  PrDiffPane,
  prDiffMergeCenter,
  type PrMergeState,
} from "../../ships/surfaces/diffview"
import { PhoneChassis } from "../surfaces/steerphone"
import { IssueScreen } from "../surfaces/mobileui"
import { ReviewPhoneScreen } from "../surfaces/reviewphone"
import {
  CL,
  CL_BOARD,
  CL_DIFF_FILES,
  CL_DIFF_ROWS,
  CL_FILE_STATS,
  CL_ISSUE,
  CL_LABELS,
  CL_PR_HEAD,
  COPY,
  NEW_ISSUE_ID,
  PHONE_START,
  REPORT,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP_EASE,
  CONTENT_TOP,
  SegmentShell,
  captionSize,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["review-merge"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  statsRoll: 14,
  paint: 18, // the diff paints into the PrDiff screen
  mergeHover: 96,
  mobileTap: 100, // portrait: the Merge capsule press on the phone's Review screen
  confirmAt: 106, // click 1 → Confirm merge (portrait: the alert is up)
  mergingAt: 120, // click 2 → Merging… (portrait: the alert's Merge tap)
  mergedAt: 138, // the phone card flips Merged · chip lands on Done
  rowFadeFrom: 148,
  rowFadeTo: 162,
} as const

const CAPTIONS = {
  rm1: { in: 12, out: 88 },
  rm2: { in: 126, out: 200 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
// ONE framing holds reviews sidebar + diff + phone for the whole clip
// (EXP-388: no camera moves).
const CAMERA_KEYS: CamKey[] = [{ f: 0, s: 1.12, x: 850, y: 470 }]

// Portrait (FEED-20, replacing the EXP-482 diff-column cut): ONE framing of
// the WHOLE phone for the whole clip. The phone renders the mobile Review
// screen, so both the diff ("Review it in place.") and the merge buttons at
// the bottom of the screen ("Merge. Done.") are on camera the entire time —
// the desktop window stays a backdrop, no camera moves (EXP-388 rule).
const CAMERA_KEYS_PT: CamKey[] = shotKeys([
  { at: 0, s: 1.8, x: 1444, y: 569 }, // the WHOLE phone: review → Merged
])

// ── Cursor ────────────────────────────────────────────────────────────────────
// Derived from the PrDiff header cluster (EXP-706) so the pointer keeps
// landing on the capsule as it morphs Merge → Confirm merge.
const MERGE_BTN = prDiffMergeCenter(CENTER_X, CONTENT_TOP, CENTER_W, `rest`)
const CONFIRM_BTN = prDiffMergeCenter(
  CENTER_X,
  CONTENT_TOP,
  CENTER_W,
  `confirm`
)

const CURSOR_KEYS: CursorKey[] = [
  { f: 74, x: 900, y: 400 },
  { f: 90, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 104, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 112, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 132, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 152, x: 780, y: 480 },
]
const CURSOR_CLICKS = [B.confirmAt, B.mergingAt]

const TAB_151: ChromeTab = {
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
}

// Phone placement in COMP coordinates inside the camera layer.
const PHONE_POS = { x: 1490, y: 280, scale: 1 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const ReviewMergeSegment: React.FC<SegmentProps> = ({
  frame,
  portrait,
}) => {
  const dockH = WIN.dockStrip
  const paneH = WIN.h - CONTENT_TOP - dockH
  const capSize = captionSize(portrait)

  const heroStatus =
    frame >= B.mergedAt ? ("done" as const) : ("in_progress" as const)

  const mergeState: PrMergeState =
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
  const phoneRise = interpolate(frame, [4, 18], [0, 1], CLAMP_EASE)

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={portrait ? CAMERA_KEYS_PT : CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TitleBar
              frame={frame}
              tabs={[{ ...TAB_151, status: heroStatus }]}
              activeId="exp151"
            />
            <ExpandedRail
              frame={frame}
              active="board"
              dots={["reviews"]}
              dotColor={C.green}
              boardName={CL.project}
              userName={CL.user}
              userInitial={CL.initials}
            />

            {/* sidebar: the board's issue list — the PrDiff is a CENTER
                screen, so the list pane behind it stays put (EXP-706) */}
            <SidebarPane actions={<BoardActions />} bottomInset={dockH}>
              <BoardTool
                frame={frame}
                rows={CL_BOARD}
                overrides={{
                  [NEW_ISSUE_ID]: {
                    status: frame >= B.mergedAt ? "done" : "in_progress",
                  },
                }}
                selectedId={NEW_ISSUE_ID}
              />
            </SidebarPane>

            {/* center: the PrDiff screen (what a Reviews row opens) */}
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
              <PrDiffPane
                frame={frame}
                paintAt={B.paint}
                statsRollAt={B.statsRoll}
                scrollY={0}
                head={CL_PR_HEAD}
                files={CL_DIFF_FILES}
                rows={CL_DIFF_ROWS}
                fileStats={CL_FILE_STATS}
                mergeState={mergeState}
                mergeMorphAt={mergeMorphAt}
                mergeHover={frame >= B.mergeHover && frame < B.confirmAt}
              />
            </div>

            <DockCollapsedStrip frame={frame} count={2} />

            <CursorLayer
              keys={CURSOR_KEYS}
              clicks={CURSOR_CLICKS}
              frame={frame}
              from={74}
              to={170}
            />
          </WindowChassis>

          {/* the phone: wide — the real mobile issue detail, the merge lands
              there as "Merged" + Done, live; portrait — the mobile Review
              screen, the merge RUNS there (FEED-20) */}
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
                <PhoneChassis glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}>
                  {portrait ? (
                    <ReviewPhoneScreen
                      frame={frame}
                      paintAt={B.paint}
                      mergeTapAt={B.mobileTap}
                      confirmAt={B.confirmAt}
                      mergingAt={B.mergingAt}
                      mergedAt={B.mergedAt}
                    />
                  ) : (
                    <IssueScreen
                      frame={frame}
                      identifier={CL_ISSUE.id}
                      title={CL_ISSUE.title}
                      origin="Feedback widget"
                      status="in_progress"
                      statusLabel="In Progress"
                      priorityLabel="No priority"
                      assignee={{ name: CL.user, initials: CL.initials }}
                      due={CL_ISSUE.due}
                      labelChip={CL_LABELS.widget}
                      description={REPORT.details}
                      activity={[
                        { text: "Feedback widget created the issue · 1 hr ago" },
                        {
                          status: "in_progress",
                          text: "Riley Chen changed status from Backlog to In Progress · 30 min ago",
                        },
                      ]}
                      pr={{
                        number: CL.pr,
                        device: PHONE_START.device,
                        user: CL.user.split(" ")[0],
                        mergedAt: B.mergedAt,
                      }}
                      sessionLive
                    />
                  )}
                </PhoneChassis>
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
