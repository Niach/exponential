// scenes/Film.tsx — the continuous f0–1380 film. Rendered inside a Sequence
// from=0, so useCurrentFrame() IS the composition-global frame every surface
// expects. Two shell phases share the WindowChassis: the S1/S2 flash-forward
// (hard-cut away at f180) and the main story shell (S3–S11).

import React from "react"
import { AbsoluteFill, interpolate, spring, useCurrentFrame } from "remotion"
import { C, EASE, SETTLE, WIN } from "../theme"
import { BOARD, COPY, HERO, HERO_SESSION, type BoardRow, type IssueStatus } from "../fixtures"
import { Camera, Caption, CursorLayer, Punch, WindowChassis, WordmarkChip } from "../rig"
import { BoardActions, BoardTool, ReviewsTool, SidebarPane, type MergeState } from "../surfaces/board"
import { CenterEmptyState, DockCollapsedStrip, IconRail, TabsBar, TopBar, type ChromeTab, type IconRailProps } from "../surfaces/chrome"
import { IssueDetailPane } from "../surfaces/detail"
import { DialogScrim, IssueCodingDialog, ReleaseCodingDialog } from "../surfaces/dialogs"
import { ChangesPane } from "../surfaces/diffview"
import { FlowGraph } from "../surfaces/flowgraph"
import { PhonePiP } from "../surfaces/phone"
import { ReleaseDetailTool, ReleasesTool } from "../surfaces/releases"
import { TerminalDock, type DockTab } from "../surfaces/terminal"
import {
  CAMERA_KEYS,
  CAPTIONS,
  CASCADE_DONE_AT,
  CURSOR_L1,
  CURSOR_L1_KEYS,
  CURSOR_L2,
  CURSOR_L2_KEYS,
  CURSOR_L3,
  CURSOR_L3_KEYS,
  DOCK_COLLAPSE_END,
  EFFORT_MENU,
  FLOW_SCHEDULE,
  MODEL_MENU,
  ORCH_EVENTS,
  ORCH_SCHEDULE,
  PHONE_SCHEDULE,
  PR_DOT_AT,
  PROGRESS_STEPS,
  S1_FEED_SCHEDULE,
  S6_EXIT_AT,
  S6_FEED_SCHEDULE,
  SCENE,
  SHIPPED_AT,
  STATUS_FLIPS,
  dockHeightAt,
  whipBlurAt,
} from "./timeline"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

// Center-pane geometry (window-local)
const CENTER_X = WIN.rail + WIN.sidebar // 304
const CENTER_W = WIN.w - CENTER_X // 1264
const CONTENT_TOP = WIN.topBar + WIN.dockTabs // 67 (below the center tab strip)

// ── Static fixtures for the shells ────────────────────────────────────────────
const TAB_EXP: ChromeTab = { id: "exp142", label: HERO.id, mono: true }
const TAB_REL: ChromeTab = { id: "rel", label: "Release v0.12" }
const CENTER_POP_AT: Record<string, number> = { exp142: SCENE.s4, rel: SCENE.s10 }

const FF_OVERRIDES: Record<string, Partial<BoardRow>> = { [HERO.id]: { status: "in_progress" } }
// zsh first — matches S6, where the session tab pops in NEXT TO the pre-existing
// zsh tab (the flash-forward shows that same future state).
const FF_DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  { id: "hero", label: HERO.sessionTab, dot: C.green },
]
const S6_DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  { id: "hero", label: HERO.sessionTab, dot: C.green, popAt: 531 },
]
const ORCH_DOCK_TABS: DockTab[] = [
  { id: "orch", label: "Release v0.12", dot: C.green, popAt: 1086 },
  { id: "zsh", label: "zsh" },
  { id: "w139", label: "claude · EXP-139", dot: C.green, popAt: 1122 },
  { id: "w141", label: "claude · EXP-141", dot: C.green, popAt: 1125 },
  { id: "w143", label: "claude · EXP-143", dot: C.green, popAt: 1128 },
  { id: "w144", label: "claude · EXP-144", dot: C.green, popAt: 1194 },
  { id: "w145", label: "claude · EXP-145", dot: C.green, popAt: 1197 },
]

// ── Flash-forward shell (S1 + S2, hard-cut at f180) ───────────────────────────
const FlashForwardShell: React.FC<{ frame: number }> = ({ frame }) => (
  <>
    <TopBar frame={frame} />
    <IconRail frame={frame} active="issues" />
    <TabsBar frame={frame} tabs={[TAB_EXP]} activeId="exp142" />
    <SidebarPane title="All Issues" actions={<BoardActions />} pills bottomInset={WIN.dockExpanded}>
      <BoardTool frame={frame} rows={BOARD} overrides={FF_OVERRIDES} showLabels={false} />
    </SidebarPane>
    <div
      style={{
        position: "absolute",
        left: CENTER_X,
        top: CONTENT_TOP,
        width: CENTER_W,
        height: WIN.h - CONTENT_TOP - WIN.dockExpanded,
        overflow: "hidden",
      }}
    >
      <IssueDetailPane
        frame={frame}
        status="in_progress"
        codingNow={{ at: 105 }}
        prChip={{ at: 158 }}
        width={CENTER_W}
        height={WIN.h - CONTENT_TOP - WIN.dockExpanded}
      />
    </div>
    <TerminalDock
      frame={frame}
      height={WIN.dockExpanded}
      tabs={FF_DOCK_TABS}
      activeTab="hero"
      feed={{ events: HERO_SESSION, schedule: S1_FEED_SCHEDULE }}
    />
  </>
)

// ── Main story shell (S3–S11) ─────────────────────────────────────────────────
const MainShell: React.FC<{ frame: number; dockH: number }> = ({ frame, dockH }) => {
  const paneH = WIN.h - CONTENT_TOP - dockH

  // Board state over the film.
  const heroStatus: IssueStatus = frame >= 904 ? "done" : frame >= 519 ? "in_progress" : "todo"
  const overrides: Record<string, Partial<BoardRow>> = { [HERO.id]: { status: heroStatus } }
  const regroup =
    frame >= 904
      ? { id: HERO.id, t: interpolate(frame, [904, 922], [0, 1], CLAMP), from: "in_progress" as IssueStatus }
      : undefined

  // Sidebar crossfades: board ↔ reviews (S8), then hard switch to releases (S9 whip).
  const boardO =
    frame < 862 ? interpolate(frame, [822, 828], [1, 0], CLAMP) : interpolate(frame, [902, 908], [0, 1], CLAMP)
  const reviewsO =
    interpolate(frame, [822, 828], [0, 1], CLAMP) * interpolate(frame, [902, 908], [1, 0], CLAMP)

  // Reviews merge-button state machine (S8).
  const mergeState: MergeState = frame < 855 ? "rest" : frame < 882 ? "confirm" : frame < 902 ? "merging" : "gone"
  const mergeMorphAt = mergeState === "confirm" ? 855 : mergeState === "merging" ? 882 : undefined
  const rowFade = interpolate(frame, [890, 902], [0, 1], CLAMP_EASE)

  // Rail active tool.
  const railProps: IconRailProps =
    frame < 820
      ? { frame, active: "issues" }
      : frame < SCENE.s9
        ? { frame, active: "reviews", activeTransition: { from: "issues", at: 820 } }
        : { frame, active: "releases", activeTransition: { from: "reviews", at: SCENE.s9 } }
  const railDots = frame < 822 ? ["reviews"] : []

  // Center tab strip.
  const centerTabs: ChromeTab[] = frame < SCENE.s4 ? [] : frame < SCENE.s10 ? [TAB_EXP] : [TAB_EXP, TAB_REL]
  const centerActive = frame < SCENE.s10 ? "exp142" : "rel"

  return (
    <>
      <TopBar frame={frame} />
      <IconRail {...railProps} dots={railDots} />
      <TabsBar frame={frame} tabs={centerTabs} activeId={centerActive} popAt={CENTER_POP_AT} />

      {/* ── Sidebar: board (S3–S8) ── */}
      {frame < SCENE.s9 ? (
        <div style={{ opacity: boardO }}>
          <SidebarPane title="All Issues" actions={<BoardActions />} pills bottomInset={dockH}>
            <BoardTool
              frame={frame}
              rows={BOARD}
              overrides={overrides}
              cascadeAt={SCENE.s3}
              hover={{ id: HERO.id, from: 240, to: 276 }}
              selectedId={frame >= 276 ? HERO.id : undefined}
              prDotId={{ id: HERO.id, at: PR_DOT_AT }}
              regroup={regroup}
              showLabels={false}
            />
          </SidebarPane>
        </div>
      ) : null}

      {/* ── Sidebar: reviews (S8) ── */}
      {frame >= 816 && frame < 914 ? (
        <div style={{ opacity: reviewsO }}>
          <SidebarPane title="Reviews" bottomInset={dockH}>
            <ReviewsTool
              frame={frame}
              mergeState={mergeState}
              morphAt={mergeMorphAt}
              hover={frame >= 846 && frame < 855}
              rowFade={rowFade}
            />
          </SidebarPane>
        </div>
      ) : null}

      {/* ── Sidebar: releases (S9–S11) ── */}
      {frame >= SCENE.s9 ? (
        <div
          style={{
            position: "absolute",
            left: WIN.rail,
            top: WIN.topBar,
            width: WIN.sidebar,
            height: WIN.h - WIN.topBar - dockH,
            backgroundColor: C.panel,
            borderRight: `1px solid ${C.border}`,
            overflow: "hidden",
          }}
        >
          <ReleasesTool frame={frame} hover={{ at: 946, out: 954 }} exitAt={956} />
          <ReleaseDetailTool
            frame={frame}
            drillAt={956}
            progress={PROGRESS_STEPS}
            statusFlipAt={STATUS_FLIPS}
            shippedAt={SHIPPED_AT}
            prChip={{ at: 1256, mergedAt: 1272 }}
            cascadeDoneAt={CASCADE_DONE_AT}
            hoverStartCoding={{ at: 976, out: 986 }}
          />
        </div>
      ) : null}

      {/* ── Center: empty state (S3) ── */}
      {frame < 310 ? (
        <div style={{ opacity: interpolate(frame, [SCENE.s4, SCENE.s4 + 6], [1, 0], CLAMP) }}>
          <CenterEmptyState frame={frame} bottom={WIN.dockStrip} />
        </div>
      ) : null}

      {/* ── Center: issue detail + Changes tab (S4–S9) ── */}
      {frame >= SCENE.s4 && frame < 1092 ? (
        <div
          style={{
            position: "absolute",
            left: CENTER_X,
            top: CONTENT_TOP,
            width: CENTER_W,
            height: paneH,
            overflow: "hidden",
            opacity: interpolate(frame, [SCENE.s10, SCENE.s10 + 8], [1, 0], CLAMP),
          }}
        >
          <IssueDetailPane
            frame={frame}
            tab={frame >= SCENE.s7 ? "changes" : "details"}
            tabSwitchAt={frame >= SCENE.s7 ? SCENE.s7 : undefined}
            slideInAt={SCENE.s4}
            staggerAt={306}
            startHover={{ at: 374, out: 391 }}
            codingNow={{ at: 531, out: S6_EXIT_AT }}
            prChip={{ at: PR_DOT_AT }}
            status={heroStatus}
            width={CENTER_W}
            height={paneH}
          />
          {frame >= SCENE.s7 ? (
            <div
              style={{
                position: "absolute",
                left: 0,
                top: 34,
                right: 0,
                bottom: 0,
                opacity: interpolate(frame, [SCENE.s7, SCENE.s7 + 6], [0, 1], CLAMP),
              }}
            >
              <ChangesPane frame={frame} paintAt={716} statsRollAt={713} fileSelectAt={752} scrollY={0} />
            </div>
          ) : null}
        </div>
      ) : null}

      {/* ── Center: branch flow graph (S10–S11) ── */}
      {frame >= SCENE.s10 ? (
        <div
          style={{
            position: "absolute",
            left: CENTER_X,
            top: CONTENT_TOP,
            width: CENTER_W,
            height: paneH,
            overflow: "hidden",
            opacity: interpolate(frame, [SCENE.s10, SCENE.s10 + 8], [0, 1], CLAMP),
          }}
        >
          <FlowGraph frame={frame} schedule={FLOW_SCHEDULE} width={CENTER_W} height={673} padTop={214} />
        </div>
      ) : null}

      {/* ── Dock ── */}
      {frame < SCENE.s6 ? (
        <DockCollapsedStrip frame={frame} count={1} />
      ) : frame < DOCK_COLLAPSE_END ? (
        <TerminalDock
          frame={frame}
          height={dockH}
          tabs={S6_DOCK_TABS}
          activeTab={frame < 531 ? "zsh" : "hero"}
          feed={{ events: HERO_SESSION, schedule: S6_FEED_SCHEDULE }}
          inputGlow={660}
          exitAt={S6_EXIT_AT}
          exitBadgeTab="hero"
        />
      ) : frame < SCENE.s10 ? (
        <DockCollapsedStrip frame={frame} count={2} />
      ) : (
        <TerminalDock
          frame={frame}
          height={dockH}
          tabs={ORCH_DOCK_TABS}
          activeTab="orch"
          feed={{ events: ORCH_EVENTS, schedule: ORCH_SCHEDULE }}
          spinnerTip={null}
        />
      )}

      {/* ── Dialogs (window-local, above everything but the cursor) ── */}
      <DialogScrim frame={frame} in={SCENE.s5} out={516} />
      {frame >= SCENE.s5 && frame < 530 ? (
        <IssueCodingDialog
          frame={frame}
          appearAt={SCENE.s5}
          modelMenu={MODEL_MENU}
          effortMenu={EFFORT_MENU}
          planCheckAt={472}
          buttonState={{ hoverAt: 500, startingAt: 516 }}
          collapseAt={516}
        />
      ) : null}
      <DialogScrim frame={frame} in={986} out={1072} />
      {frame >= 988 && frame < 1085 ? (
        <ReleaseCodingDialog
          frame={frame}
          appearAt={988}
          checkShimmerAt={1002}
          selectsFlickAt={1014}
          ultraPulseAt={1040}
          buttonState={{ hoverAt: 1060, startingAt: 1068 }}
          collapseAt={1072}
        />
      ) : null}
    </>
  )
}

// ── Phone PiP (screen-space; slides in f618–633, exits f690–702) ─────────────
const PhoneLayer: React.FC<{ frame: number }> = ({ frame }) => {
  if (frame < 616 || frame > 704) return null
  const slideIn = frame < 618 ? 0 : spring({ frame: frame - 618, fps: 30, config: SETTLE })
  const exitT = interpolate(frame, [690, 702], [0, 1], CLAMP_EASE)
  return (
    <PhonePiP
      frame={frame}
      x={2020 - 575 * slideIn + 240 * exitT}
      y={165}
      rotate={2 - 2 * slideIn}
      feedSchedule={PHONE_SCHEDULE}
      sendPulseAt={660}
      opacity={1 - exitT}
    />
  )
}

// ── The film ──────────────────────────────────────────────────────────────────
export const Film: React.FC = () => {
  const frame = useCurrentFrame() // global (Sequence from=0)
  const dockH = dockHeightAt(frame)
  const blur = whipBlurAt(frame)
  const dim = interpolate(frame, [1330, 1345], [0, 0.2], CLAMP)

  return (
    <AbsoluteFill>
      {/* camera layer (whip-pan blur wraps it) */}
      <AbsoluteFill style={{ filter: blur > 0.05 ? `blur(${blur.toFixed(2)}px)` : undefined }}>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis dim={dim}>
            {frame < SCENE.s3 ? <FlashForwardShell frame={frame} /> : <MainShell frame={frame} dockH={dockH} />}
            {/* window-local cursor choreography */}
            <CursorLayer keys={CURSOR_L1_KEYS} clicks={[...CURSOR_L1.clicks]} frame={frame} from={CURSOR_L1.from} to={CURSOR_L1.to} />
            <CursorLayer keys={CURSOR_L2_KEYS} clicks={[...CURSOR_L2.clicks]} frame={frame} from={CURSOR_L2.from} to={CURSOR_L2.to} />
            <CursorLayer keys={CURSOR_L3_KEYS} clicks={[...CURSOR_L3.clicks]} frame={frame} from={CURSOR_L3.from} to={CURSOR_L3.to} />
          </WindowChassis>
        </Camera>
      </AbsoluteFill>

      {/* screen-space layer (NOT under the camera) */}
      <PhoneLayer frame={frame} />
      <Punch frame={frame} in={CAPTIONS.hook.in} out={CAPTIONS.hook.out} lines={[COPY.hook1, COPY.hook2]} />
      <Caption frame={frame} in={CAPTIONS.s2.in} out={CAPTIONS.s2.out} size={40}>
        {COPY.s2}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s3.in} out={CAPTIONS.s3.out}>
        {COPY.s3}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s5.in} out={CAPTIONS.s5.out}>
        {COPY.s5}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s6a.in} out={CAPTIONS.s6a.out}>
        {COPY.s6a}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s6b.in} out={CAPTIONS.s6b.out}>
        {COPY.s6b}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s7.in} out={CAPTIONS.s7.out}>
        {COPY.s7}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s8.in} out={CAPTIONS.s8.out}>
        {COPY.s8}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s9.in} out={CAPTIONS.s9.out}>
        {COPY.s9}
      </Caption>
      <Caption frame={frame} in={CAPTIONS.s10.in} out={CAPTIONS.s10.out}>
        {COPY.s10}
      </Caption>
      <Punch frame={frame} in={CAPTIONS.s11.in} out={CAPTIONS.s11.out} lines={[COPY.s11]} size={72} weight={700} y={900} />
      <WordmarkChip frame={frame} in={CAPTIONS.wordmark.in} out={CAPTIONS.wordmark.out} />
    </AbsoluteFill>
  )
}
