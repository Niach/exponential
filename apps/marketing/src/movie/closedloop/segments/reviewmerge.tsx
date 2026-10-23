// closedloop/segments/reviewmerge.tsx — clip 3 (235f, refocused by EXP-388):
// ONE surface, one framing. The thread's Guide › Changes page (EXP-895 the
// ONE diff view: the file tree beside the unified diff cards) paints in the
// panel, and the thread header carries the ONE merge control while the
// Guide's PR is open (thread_screen.rs: `Merge PR`, two-click via
// pr_merge.rs: Merge PR → Confirm merge → Merging…). The PR opened moved
// EXP-151 to In Review (issue-lifecycle `pr_opened`); the merge moves it to
// Done (`pr_merged`) and the pill leaves with the open PR. The phone beside
// the window shows the REAL mobile issue page landing on Done.
// PORTRAIT: the phone IS the clip — the thread's Guide › Changes page on
// iOS with its Merge PR capsule; the merge runs THERE.
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { PAGE_FONT } from "../../ships/theme"
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
  CutoutPanel,
  PANEL,
  Sidebar,
  TitleBar,
  panelPt,
} from "../../ships/surfaces/shell"
import {
  ChangesView,
  ThreadHeader,
  type ChangesFile,
  type MergePill,
} from "../../ships/surfaces/threadview"
import { PhoneChassis } from "../surfaces/steerphone"
import { IssueScreen } from "../surfaces/mobileui"
import { ReviewPhoneScreen } from "../surfaces/reviewphone"
import {
  CL,
  CL_BOARDS,
  CL_DIFF_ROWS,
  CL_ISSUE,
  CL_LABELS,
  CL_THREAD_TITLE,
  COPY,
  NEW_ISSUE_ID,
  PHONE_START,
  REPORT,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CLAMP_EASE,
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
// ONE framing holds the sidebar + the Changes page + the phone for the whole clip
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

// ── The Changes page's files (the PR = the session's two edits) ─────────────
const PB = "apps/shop/src/checkout/PayButton.tsx"
const TEST = "apps/shop/src/checkout/pay-button.test.tsx"
const FILES: ChangesFile[] = [
  {
    path: PB,
    status: "M",
    add: 27,
    del: 6,
    unchangedAbove: 20,
    lines: CL_DIFF_ROWS.slice(0, 18).map((r) => ({ ...r })),
  },
  {
    path: TEST,
    status: "A",
    add: 42,
    del: 0,
    lines: [
      { t: "hunk", text: "@@ -0,0 +1,42 @@" },
      { t: "add", text: 'import { fireEvent, render } from "@testing-library/react"', new: 1 },
      { t: "add", text: 'import { PayButton } from "./PayButton"', new: 2 },
    ],
  },
]

// ── Cursor ────────────────────────────────────────────────────────────────────
// The merge pill sits left of the header's `…` (right inset 44, 24 + gap 7);
// the pointer follows its centre as it morphs Merge PR → Confirm merge.
const PILL_RIGHT = PANEL.w - 2 - 44 - 24 - 7
const MERGE_BTN = panelPt(PILL_RIGHT - 46, 37)
const CONFIRM_BTN = panelPt(PILL_RIGHT - 56, 37)

const CURSOR_KEYS: CursorKey[] = [
  { f: 74, x: 900, y: 400 },
  { f: 90, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 104, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 112, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 132, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 152, x: 780, y: 480 },
]
const CURSOR_CLICKS = [B.confirmAt, B.mergingAt]

// Phone placement in COMP coordinates inside the camera layer.
const PHONE_POS = { x: 1490, y: 280, scale: 1 } as const


// ── The clip ──────────────────────────────────────────────────────────────────
export const ReviewMergeSegment: React.FC<SegmentProps> = ({
  frame,
  portrait,
}) => {
  const capSize = captionSize(portrait)
  const merged = frame >= B.mergedAt
  const mergeState: MergePill | undefined =
    frame < B.confirmAt
      ? "merge"
      : frame < B.mergingAt
        ? "confirm"
        : frame < B.mergedAt
          ? "merging"
          : undefined
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
              tabs={[
                // the turn ended with the PR: session-card `waiting`
                { id: "tab", lead: { kind: "waiting" }, title: CL_THREAD_TITLE },
              ]}
              activeId="tab"
            />
            <Sidebar
              frame={frame}
              team={CL.team}
              user={CL.user.split(" ")[0]}
              initials={CL.initials}
              selected="thread:exp151"
              inboxDotAt={0}
              boards={[...CL_BOARDS]}
              active={[{ id: "exp151", title: CL_THREAD_TITLE }]}
            />
            <CutoutPanel>
              <ThreadHeader
                frame={frame}
                title={CL_THREAD_TITLE}
                issue={{
                  id: NEW_ISSUE_ID,
                  title: CL_ISSUE.title,
                  status: merged ? "done" : "in_review",
                }}
                merge={mergeState ? { state: mergeState, at: mergeMorphAt } : undefined}
                face="guide"
                width={PANEL.w - 2}
              />
              <ChangesView
                frame={frame}
                files={FILES}
                width={PANEL.w - 2}
                height={PANEL.h - 2}
                paintAt={B.paint}
              />
            </CutoutPanel>

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
                      status="in_review"
                      statusLabel="In Review"
                      priorityLabel="No priority"
                      assignee={{ name: CL.user, initials: CL.initials }}
                      due={CL_ISSUE.due}
                      labelChip={CL_LABELS.widget}
                      description={REPORT.details}
                      activity={[
                        { text: "Feedback widget created the issue · 1 hr ago" },
                      ]}
                      pr={{
                        number: CL.pr,
                        device: PHONE_START.device,
                        user: CL.user.split(" ")[0],
                        mergedAt: B.mergedAt,
                      }}
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
