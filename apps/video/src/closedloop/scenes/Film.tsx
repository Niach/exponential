// closedloop/scenes/Film.tsx — the continuous loop film (780 story frames +
// the END_HOLD rest tail, see timeline.ts). Two shells share the camera: the
// light acme.shop browser (S1/S2 and the closing S9) and the dark Exponential
// app window (S3–S8). useCurrentFrame() IS the composition-global frame (the
// comp renders this directly at from=0). `textScale` multiplies ONLY the
// screen-space Caption/Punch sizes (EXP-176 phone legibility) — never the
// in-window UI, whose px values are contract-locked (ships/CONTRACT.md).

import React from "react";
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import { C, EASE, WIN } from "../../ships/theme";
import {
  Camera,
  Caption,
  CursorLayer,
  Punch,
  WindowChassis,
} from "../../ships/rig";
import {
  BoardActions,
  BoardTool,
  ReviewsTool,
  SidebarPane,
  type MergeState,
} from "../../ships/surfaces/board";
import {
  CenterEmptyState,
  DockCollapsedStrip,
  IconRail,
  TabsBar,
  TopBar,
  type ChromeTab,
  type IconRailProps,
  type RailIconId,
} from "../../ships/surfaces/chrome";
import { IssueDetailPane } from "../../ships/surfaces/detail";
import { DialogScrim } from "../../ships/surfaces/dialogs";
import { ChangesPane } from "../../ships/surfaces/diffview";
import { TerminalDock, type DockTab } from "../../ships/surfaces/terminal";
import {
  CL,
  CL_BOARD,
  CL_DIFF_FILES,
  CL_DIFF_HEADER,
  CL_DIFF_ROWS,
  CL_FILE_STATS,
  CL_ISSUE,
  CL_REVIEW_ROW,
  CL_SESSION,
  COPY,
  NEW_ISSUE_ID,
} from "../fixtures";
import {
  BrowserChassis,
  EmailCard,
  FeedbackFab,
  SiteViewport,
} from "../surfaces/sitemock";
import { WidgetPanel } from "../surfaces/widgetmock";
import { StartCodingDialog } from "../surfaces/startdialog";
import {
  BOARD_BEATS,
  CAMERA_KEYS,
  CAPTIONS,
  CODING_PILL,
  CODING_START,
  CURSOR_APP1,
  CURSOR_APP1_KEYS,
  CURSOR_APP2,
  CURSOR_APP2_KEYS,
  CURSOR_END,
  CURSOR_END_KEYS,
  CURSOR_SITE,
  CURSOR_SITE_KEYS,
  DIALOG_BEATS,
  DIFF_BEATS,
  DOCK_COLLAPSE_END,
  EMAIL_BEATS,
  FEED_SCHEDULE,
  MERGE_BEATS,
  PR_AT,
  SCENE,
  SESSION_EXIT,
  SESSION_TAB_POP,
  SITE_BEATS,
  SPINNER_BASE,
  dockHeightAt,
  whipBlurAt,
} from "../timeline";

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const;

// Center-pane geometry (window-local).
const CENTER_X = WIN.rail + WIN.sidebar; // 304
const CENTER_W = WIN.w - CENTER_X; // 1264
const CONTENT_TOP = WIN.topBar + WIN.dockTabs; // 67

// The ClosedLoop rail set: NO rocket/releases icon (deleted feature).
const RAIL_IDS: RailIconId[] = [
  "search",
  "inbox",
  "agents",
  "issues",
  "reviews",
  "files",
  "source-control",
  "settings",
  "account",
];

const TAB_151: ChromeTab = { id: "exp151", label: NEW_ISSUE_ID, mono: true };
const TAB_POP: Record<string, number> = { exp151: BOARD_BEATS.tabPop };

const DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  { id: "cl", label: CL.sessionTab, dot: C.green, popAt: SESSION_TAB_POP },
];

// ── The acme.shop shell (S1/S2 + S9) ──────────────────────────────────────────
const SiteShell: React.FC<{ frame: number }> = ({ frame }) => {
  const opening = frame < SCENE.board;
  return (
    <BrowserChassis>
      <SiteViewport
        frame={frame}
        shakeAts={opening ? [SITE_BEATS.payClick1, SITE_BEATS.payClick2] : []}
      />
      <FeedbackFab
        frame={frame}
        hoverAt={opening ? SITE_BEATS.fabHover : undefined}
        pressAt={opening ? SITE_BEATS.fabClick : undefined}
        restAt={opening ? SITE_BEATS.fabRest : undefined}
      />
      {opening && frame >= SITE_BEATS.panelAppear ? (
        <WidgetPanel
          frame={frame}
          appearAt={SITE_BEATS.panelAppear}
          annotateAt={SITE_BEATS.annotate}
          titleTypeAt={SITE_BEATS.titleType}
          detailsTypeAt={SITE_BEATS.detailsType}
          sendHoverAt={SITE_BEATS.sendHover}
          sendingAt={SITE_BEATS.sending}
          successAt={SITE_BEATS.success}
        />
      ) : null}
      {!opening ? (
        <EmailCard
          frame={frame}
          appearAt={EMAIL_BEATS.appear}
          fadeAt={EMAIL_BEATS.fade}
        />
      ) : null}
      {/* window-local cursor choreography */}
      <CursorLayer
        keys={CURSOR_SITE_KEYS}
        clicks={[...CURSOR_SITE.clicks]}
        frame={frame}
        from={CURSOR_SITE.from}
        to={CURSOR_SITE.to}
      />
      <CursorLayer
        keys={CURSOR_END_KEYS}
        clicks={CURSOR_END.clicks}
        frame={frame}
        from={CURSOR_END.from}
        to={CURSOR_END.to}
      />
    </BrowserChassis>
  );
};

// ── The Exponential app shell (S3–S8) ─────────────────────────────────────────
const AppShell: React.FC<{ frame: number; dockH: number }> = ({
  frame,
  dockH,
}) => {
  const paneH = WIN.h - CONTENT_TOP - dockH;

  // Board state over the film.
  const heroStatus =
    frame >= MERGE_BEATS.doneAt
      ? ("done" as const)
      : frame >= CODING_START
        ? ("in_progress" as const)
        : ("todo" as const);
  const overrides = { [NEW_ISSUE_ID]: { status: heroStatus } };
  const regroup =
    frame >= MERGE_BEATS.doneAt
      ? {
          id: NEW_ISSUE_ID,
          t: interpolate(
            frame,
            [MERGE_BEATS.doneAt, MERGE_BEATS.regroupEnd],
            [0, 1],
            CLAMP,
          ),
          from: "in_progress" as const,
        }
      : frame >= CODING_START
        ? {
            id: NEW_ISSUE_ID,
            t: interpolate(
              frame,
              [CODING_START, CODING_START + 16],
              [0, 1],
              CLAMP,
            ),
            from: "todo" as const,
          }
        : undefined;

  // Sidebar crossfades: board ↔ reviews (S8).
  const boardO =
    frame < 700
      ? interpolate(
          frame,
          [MERGE_BEATS.sidebarSwapOut, MERGE_BEATS.sidebarSwapOut + 6],
          [1, 0],
          CLAMP,
        )
      : interpolate(
          frame,
          [MERGE_BEATS.sidebarSwapIn, MERGE_BEATS.sidebarSwapIn + 6],
          [0, 1],
          CLAMP,
        );
  const reviewsO =
    interpolate(
      frame,
      [MERGE_BEATS.sidebarSwapOut, MERGE_BEATS.sidebarSwapOut + 6],
      [0, 1],
      CLAMP,
    ) *
    interpolate(
      frame,
      [MERGE_BEATS.sidebarSwapIn, MERGE_BEATS.sidebarSwapIn + 6],
      [1, 0],
      CLAMP,
    );

  // Reviews merge-button state machine.
  const mergeState: MergeState =
    frame < MERGE_BEATS.confirmAt
      ? "rest"
      : frame < MERGE_BEATS.mergingAt
        ? "confirm"
        : frame < MERGE_BEATS.rowFadeTo
          ? "merging"
          : "gone";
  const mergeMorphAt =
    mergeState === "confirm"
      ? MERGE_BEATS.confirmAt
      : mergeState === "merging"
        ? MERGE_BEATS.mergingAt
        : undefined;
  const rowFade = interpolate(
    frame,
    [MERGE_BEATS.rowFadeFrom, MERGE_BEATS.rowFadeTo],
    [0, 1],
    CLAMP_EASE,
  );

  // Rail.
  const railProps: IconRailProps =
    frame < MERGE_BEATS.railTransition
      ? { frame, active: "issues", icons: RAIL_IDS }
      : {
          frame,
          active: "reviews",
          activeTransition: { from: "issues", at: MERGE_BEATS.railTransition },
          icons: RAIL_IDS,
        };
  const railDots =
    frame >= PR_AT && frame < MERGE_BEATS.railClick + 4 ? ["reviews"] : [];

  return (
    <>
      <TopBar frame={frame} projectName={CL.project} runConfig={CL.runConfig} />
      <IconRail {...railProps} dots={railDots} />
      <TabsBar
        frame={frame}
        tabs={[TAB_151]}
        activeId="exp151"
        popAt={TAB_POP}
      />

      {/* ── Sidebar: board ── */}
      {frame < MERGE_BEATS.sidebarSwapOut + 8 ||
      frame >= MERGE_BEATS.sidebarSwapIn ? (
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
              cascadeAt={BOARD_BEATS.cascade}
              insertAt={{ id: NEW_ISSUE_ID, at: BOARD_BEATS.insert }}
              hover={{
                id: NEW_ISSUE_ID,
                from: BOARD_BEATS.rowHoverFrom,
                to: BOARD_BEATS.rowClick,
              }}
              selectedId={
                frame >= BOARD_BEATS.rowClick ? NEW_ISSUE_ID : undefined
              }
              prDotId={{ id: NEW_ISSUE_ID, at: PR_AT }}
              regroup={regroup}
              showLabels={false}
            />
          </SidebarPane>
        </div>
      ) : null}

      {/* ── Sidebar: reviews (S8) ── */}
      {frame >= MERGE_BEATS.railTransition &&
      frame < MERGE_BEATS.sidebarSwapIn + 8 ? (
        <div style={{ opacity: reviewsO }}>
          <SidebarPane title="Reviews" bottomInset={dockH}>
            <ReviewsTool
              frame={frame}
              mergeState={mergeState}
              morphAt={mergeMorphAt}
              hover={
                frame >= MERGE_BEATS.mergeHover && frame < MERGE_BEATS.confirmAt
              }
              rowFade={rowFade}
              row={CL_REVIEW_ROW}
              project={CL.project}
            />
          </SidebarPane>
        </div>
      ) : null}

      {/* ── Center: empty state until the issue opens ── */}
      {frame < BOARD_BEATS.tabPop + 8 ? (
        <div
          style={{
            opacity: interpolate(
              frame,
              [BOARD_BEATS.tabPop, BOARD_BEATS.tabPop + 6],
              [1, 0],
              CLAMP,
            ),
          }}
        >
          <CenterEmptyState frame={frame} bottom={WIN.dockStrip} />
        </div>
      ) : null}

      {/* ── Center: issue detail + Changes tab ── */}
      {frame >= BOARD_BEATS.tabPop ? (
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
            tab={frame >= DIFF_BEATS.tabSwitch ? "changes" : "details"}
            tabSwitchAt={
              frame >= DIFF_BEATS.tabSwitch ? DIFF_BEATS.tabSwitch : undefined
            }
            slideInAt={BOARD_BEATS.tabPop}
            staggerAt={BOARD_BEATS.detailStagger}
            startHover={{
              at: BOARD_BEATS.startHover,
              out: BOARD_BEATS.startHoverOut,
            }}
            codingNow={{ at: CODING_PILL.at, out: CODING_PILL.out }}
            prChip={{ at: PR_AT }}
            status={heroStatus}
            priority="none"
            issue={CL_ISSUE}
            showRelease={false}
            width={CENTER_W}
            height={paneH}
          />
          {frame >= DIFF_BEATS.tabSwitch ? (
            <div
              style={{
                position: "absolute",
                left: 0,
                top: 34,
                right: 0,
                bottom: 0,
                opacity: interpolate(
                  frame,
                  [DIFF_BEATS.tabSwitch, DIFF_BEATS.tabSwitch + 6],
                  [0, 1],
                  CLAMP,
                ),
              }}
            >
              <ChangesPane
                frame={frame}
                paintAt={DIFF_BEATS.paint}
                statsRollAt={DIFF_BEATS.statsRoll}
                fileSelectAt={DIFF_BEATS.fileSelect}
                scrollY={0}
                header={CL_DIFF_HEADER}
                files={CL_DIFF_FILES}
                rows={CL_DIFF_ROWS}
                fileStats={CL_FILE_STATS}
              />
            </div>
          ) : null}
        </div>
      ) : null}

      {/* ── Dock ── */}
      {frame < SCENE.dock ? (
        <DockCollapsedStrip frame={frame} count={1} />
      ) : frame < DOCK_COLLAPSE_END ? (
        <TerminalDock
          frame={frame}
          height={dockH}
          tabs={DOCK_TABS}
          activeTab={frame < SESSION_TAB_POP ? "zsh" : "cl"}
          feed={{ events: CL_SESSION, schedule: FEED_SCHEDULE }}
          exitAt={SESSION_EXIT}
          exitBadgeTab="cl"
          spinnerBase={SPINNER_BASE}
        />
      ) : (
        <DockCollapsedStrip frame={frame} count={2} />
      )}

      {/* ── Start-coding dialog ── */}
      <DialogScrim
        frame={frame}
        in={DIALOG_BEATS.appear}
        out={DIALOG_BEATS.scrimOut}
      />
      {frame >= DIALOG_BEATS.appear && frame < SCENE.dock ? (
        <StartCodingDialog
          frame={frame}
          appearAt={DIALOG_BEATS.appear}
          checkPulseAt={DIALOG_BEATS.checkPulse}
          rowHover={DIALOG_BEATS.rowHover}
          buttonState={{
            hoverAt: DIALOG_BEATS.buttonHover,
            startingAt: DIALOG_BEATS.starting,
          }}
          collapseAt={DIALOG_BEATS.collapse}
        />
      ) : null}

      {/* window-local cursor choreography */}
      <CursorLayer
        keys={CURSOR_APP1_KEYS}
        clicks={[...CURSOR_APP1.clicks]}
        frame={frame}
        from={CURSOR_APP1.from}
        to={CURSOR_APP1.to}
      />
      <CursorLayer
        keys={CURSOR_APP2_KEYS}
        clicks={[...CURSOR_APP2.clicks]}
        frame={frame}
        from={CURSOR_APP2.from}
        to={CURSOR_APP2.to}
      />
    </>
  );
};

// ── The film ──────────────────────────────────────────────────────────────────
export const Film: React.FC<{ textScale?: number }> = ({ textScale = 1 }) => {
  const frame = useCurrentFrame();
  const dockH = dockHeightAt(frame);
  const blur = whipBlurAt(frame);
  const onSite = frame < SCENE.board || frame >= SCENE.email;
  const captionSize = Math.round(44 * textScale);

  return (
    <AbsoluteFill>
      {/* camera layer (whip-pan blur wraps it) */}
      <AbsoluteFill
        style={{
          filter: blur > 0.05 ? `blur(${blur.toFixed(2)}px)` : undefined,
        }}
      >
        <Camera keys={CAMERA_KEYS} frame={frame}>
          {onSite ? (
            <SiteShell frame={frame} />
          ) : (
            <WindowChassis>
              <AppShell frame={frame} dockH={dockH} />
            </WindowChassis>
          )}
        </Camera>
      </AbsoluteFill>

      {/* screen-space captions */}
      <Caption
        frame={frame}
        in={CAPTIONS.s1.in}
        out={CAPTIONS.s1.out}
        size={captionSize}
      >
        {COPY.s1}
      </Caption>
      <Caption
        frame={frame}
        in={CAPTIONS.s2.in}
        out={CAPTIONS.s2.out}
        size={captionSize}
      >
        {COPY.s2}
      </Caption>
      <Caption
        frame={frame}
        in={CAPTIONS.s3.in}
        out={CAPTIONS.s3.out}
        size={captionSize}
      >
        {COPY.s3}
      </Caption>
      <Caption
        frame={frame}
        in={CAPTIONS.s5.in}
        out={CAPTIONS.s5.out}
        size={captionSize}
      >
        {COPY.s5}
      </Caption>
      <Caption
        frame={frame}
        in={CAPTIONS.s6.in}
        out={CAPTIONS.s6.out}
        size={captionSize}
      >
        {COPY.s6}
      </Caption>
      <Caption
        frame={frame}
        in={CAPTIONS.s7.in}
        out={CAPTIONS.s7.out}
        size={captionSize}
      >
        {COPY.s7}
      </Caption>
      <Caption
        frame={frame}
        in={CAPTIONS.s8.in}
        out={CAPTIONS.s8.out}
        size={captionSize}
      >
        {COPY.s8}
      </Caption>
      <Punch
        frame={frame}
        in={CAPTIONS.s9.in}
        out={CAPTIONS.s9.out}
        lines={[COPY.s9]}
        size={Math.round(64 * textScale)}
        weight={700}
        y={880}
      />
    </AbsoluteFill>
  );
};
