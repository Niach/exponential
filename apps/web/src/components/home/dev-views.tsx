import {
  CodeBlockSpecimen,
  FeedbackStatesSpecimen,
  TraceSpecimen,
  AskDockSpecimen,
  DockCardSpecimen,
  DeviceReadinessSpecimen,
  PlanCardSpecimen,
  PlanPageSpecimen,
  ChatListSpecimen,
  ChatSelectionSpecimen,
  ChatMenusSpecimen,
  ChatTranscriptSpecimen,
  PrChecksSpecimen,
  ResultsTabSpecimen,
  NeedsYouSpecimen,
  PhoneTabBarSpecimen,
  PhoneBoardMenuSpecimen,
  ChatHeaderSpecimen,
  PhonePageHeaderSpecimen,
  CallControlsSpecimen,
  VoiceCallPillSpecimen,
  VoiceCallSpecimen,
  VoiceCallTabsSpecimen,
  VoiceOverlayCapsuleSpecimen,
  MentionPillSpecimen,
  SchedulePickerSpecimen,
  PrRowSpecimen,
  SealedLockedSpecimen,
  useIsMobile,
} from "@exp/ui"
import { CiRunSpecimen } from "@/components/ci/ci-run-page"
import { RunnerSettingsSpecimen } from "@/components/ci/runner-section"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { isProductionBuild } from "@/lib/production-build"

// #EXP-2027: the home page's DEV specimens, shared by `/home` (md+) and the
// phones' `/t/$teamSlug/agent` (md+ forwards the latter to the former,
// keeping the search, so both read `?dev=`).
// EXP-1259 (DEV-ONLY): `?dev=feedback-states` swaps the page for the six
// feedback kinds side by side, for the view catalog's `feedback-states`
// capture; EXP-1324: `?dev=trace` / `?dev=code-block` the Trace and
// CodeBlock specimens (`?dev=ask-dock` THE ask dock's, FEED-160 `?dev=device-readiness` its specimen, EXP-1280 `?dev=plan-card`
// THE rich plan card's; chat 58cb32df `?dev=chat-list` / `chat-menus` /
// `chat-transcript` / `phone-tab-bar`, #EXP-1384 `phone-board-menu` /
// `phone-page-header`, #EXP-1388 `chat-selection` the chats.json specimens;
// S-3 `voice-call` / `voice-call-overlay` / `call-controls` the voice-call.json
// ones: web's ringing and in-call surface, the desktop pill, the round
// buttons; EXP-1681 `call-pill` THE floating call pill; #EXP-1804 `ci-run` /
// `runner-settings` the CI run page and runner section over their fixtures'
// specimens; #EXP-1611 `pr-checks` the
// Merge pill's check states + the Checks submenu; #FEED-191
// `results-tab` THE Results tab body, `needs-you` THE Needs you list; EXP-1558
// `mention-pill` / `schedule-picker` / `pr-row` / `sealed-locked` the
// styleguide boards the natives shoot as `sg_*`) (identical ×4) for theirs.
// Never parsed in a production build (`isProductionBuild`).
export const DEV_VIEWS = [
  `feedback-states`,
  `trace`,
  `code-block`,
  `ask-dock`,
  `dock-card`,
  `device-readiness`,
  `plan-card`,
  `plan-page`,
  `chat-list`,
  `chat-menus`,
  `chat-selection`,
  `chat-transcript`,
  `phone-tab-bar`,
  `phone-board-menu`,
  `phone-page-header`,
  `chat-header`,
  `voice-call`,
  `voice-call-overlay`,
  `call-controls`,
  `call-pill`,
  `pr-checks`,
  `ci-run`,
  `runner-settings`,
  `results-tab`,
  `needs-you`,
  `mention-pill`,
  `schedule-picker`,
  `pr-row`,
  `sealed-locked`,
] as const
export type DevView = (typeof DEV_VIEWS)[number]
/** EXP-1857: the web `voice-call` row of five (the fallback replaces
 *  "getting ready"); phones show all six. */
const WEB_CALL_SPECIMENS = [`ringing`, `in call, speaking`, `in call, listening`, `muted`, `in call, falling back`]

export const devView = (value: unknown): DevView | undefined =>
  !isProductionBuild && DEV_VIEWS.includes(value as DevView)
    ? (value as DevView)
    : undefined

/** The `?dev=` page: one specimen board in the page's scroller. */
export function HomeDevView({ dev }: { dev: DevView }) {
  const isMobile = useIsMobile()
  return (
    <div className={`h-full min-h-0 overflow-y-auto p-6 max-md:p-4 ${PAGE_END_CLEARANCE}`}>
      {dev === `trace` ? (
        <TraceSpecimen />
      ) : dev === `ask-dock` ? (
        <AskDockSpecimen />
      ) : dev === `dock-card` ? (
        <DockCardSpecimen />
      ) : dev === `device-readiness` ? (
        <DeviceReadinessSpecimen />
      ) : dev === `plan-card` ? (
        <PlanCardSpecimen />
      ) : dev === `plan-page` ? (
        <PlanPageSpecimen size={isMobile ? `phone` : `wide`} />
      ) : dev === `chat-list` ? (
        <ChatListSpecimen />
      ) : dev === `chat-menus` ? (
        <ChatMenusSpecimen />
      ) : dev === `chat-selection` ? (
        <ChatSelectionSpecimen />
      ) : dev === `chat-transcript` ? (
        <ChatTranscriptSpecimen single />
      ) : dev === `pr-checks` ? (
        <PrChecksSpecimen />
      ) : dev === `ci-run` ? (
        <CiRunSpecimen />
      ) : dev === `runner-settings` ? (
        <RunnerSettingsSpecimen />
      ) : dev === `results-tab` ? (
        <ResultsTabSpecimen size={isMobile ? `phone` : `wide`} />
      ) : dev === `needs-you` ? (
        <NeedsYouSpecimen />
      ) : dev === `phone-tab-bar` ? (
        <PhoneTabBarSpecimen />
      ) : dev === `phone-board-menu` ? (
        <PhoneBoardMenuSpecimen />
      ) : dev === `phone-page-header` ? (
        <PhonePageHeaderSpecimen />
      ) : dev === `chat-header` ? (
        <ChatHeaderSpecimen />
      ) : dev === `mention-pill` ? (
        <MentionPillSpecimen />
      ) : dev === `schedule-picker` ? (
        <SchedulePickerSpecimen />
      ) : dev === `pr-row` ? (
        <PrRowSpecimen />
      ) : dev === `sealed-locked` ? (
        <SealedLockedSpecimen />
      ) : dev === `code-block` ? (
        <CodeBlockSpecimen />
      ) : dev === `voice-call` ? (
        // voice-call.json `surfaceSpecimens`, each captioned: one row of
        // five on web (EXP-1857: the fallback's device line in place of
        // "getting ready", the ring's other status), all six in one
        // full-width column on phones.
        <div className="grid gap-6">
          <VoiceCallSpecimen
            platform="web"
            names={isMobile ? undefined : WEB_CALL_SPECIMENS}
            className={isMobile ? `grid grid-cols-1 gap-4` : `grid grid-cols-5 items-start gap-3`}
            frameClassName={isMobile ? `h-[640px] w-full` : `h-[704px] w-full`}
          />
          {/* FEED-272: the call's Chat and Voice tabs. */}
          <VoiceCallTabsSpecimen
            platform="web"
            className={isMobile ? `grid grid-cols-1 gap-4` : `grid grid-cols-2 items-start gap-3`}
            frameClassName={isMobile ? `h-[640px] w-full` : `h-[704px] w-full`}
          />
        </div>
      ) : dev === `voice-call-overlay` ? (
        // voice-call-overlay: the push-to-talk capsules, then the pills.
        <div className="grid gap-4">
          <VoiceOverlayCapsuleSpecimen />
          <VoiceCallPillSpecimen />
        </div>
      ) : dev === `call-controls` ? (
        <CallControlsSpecimen />
      ) : dev === `call-pill` ? (
        <VoiceCallPillSpecimen />
      ) : (
        <FeedbackStatesSpecimen />
      )}
    </div>
  )
}
