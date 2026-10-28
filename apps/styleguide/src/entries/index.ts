/**
 * EXP-1029 contract — every entry under the four sections, by fixed export
 * name. A leaf fills ITS file; nobody edits this list (EXP-1019 splices it
 * into the page when it moves the existing entries into the sections).
 */
import { entry as pickerBoard } from "./picker-board.tsx"
import { entry as pickerIssue } from "./picker-issue.tsx"
import { entry as pickerAccount } from "./picker-account.tsx"
import { entry as pickerDevice } from "./picker-device.tsx"
import { entry as pickerAssignee } from "./picker-assignee.tsx"
import { entry as pickerIcon } from "./picker-icon.tsx"
import { entry as pickerStatus } from "./picker-status.tsx"
import { entry as pickerPriority } from "./picker-priority.tsx"
import { entry as pickerLabel } from "./picker-label.tsx"
import { entry as pickerMcp } from "./picker-mcp.tsx"
import { entry as pickerRepository } from "./picker-repository.tsx"
import { entry as scopePicker } from "./scope-picker.tsx"
import { entry as schedulePicker } from "./schedule-picker.tsx"
import { entry as toast } from "./toast.tsx"
import { entry as jumpToBottom } from "./jump-to-bottom.tsx"
import { entry as stripChip } from "./strip-chip.tsx"
import { entry as mentionPill } from "./mention-pill.tsx"
import { entry as composerDialog } from "./composer-dialog.tsx"
import { entry as issueContextMenu } from "./issue-context-menu.tsx"
import { entry as sessionRow } from "./session-row.tsx"
import { entry as prRow } from "./pr-row.tsx"
import { entry as prGraphBadge } from "./pr-graph-badge.tsx"
import { entry as deviceSettings } from "./device-settings.tsx"
import { entry as workScreen } from "./work-screen.tsx"
import { entry as stackMergeChoiceDialog } from "./stack-merge-choice-dialog.tsx"
import { entry as draftLeaveDialog } from "./draft-leave-dialog.tsx"
import { entry as readinessChecklist } from "./readiness-checklist.tsx"
import { entry as deviceReadiness } from "./device-readiness.tsx"
import { entry as deviceAttention } from "./device-attention.tsx"
import { entry as resultsGuide } from "./results-guide.tsx"
import { entry as mcpAppViews } from "./mcp-app-views.tsx"
import { entry as runStatusRow } from "./run-status-row.tsx"
import { entry as sessionThread } from "./session-thread.tsx"
import { entry as composerAgentRow } from "./composer-agent-row.tsx"
import { entry as workCard } from "./work-card.tsx"
import { entry as threadParts } from "./thread-parts.tsx"
import { entry as sessionXref } from "./session-xref.tsx"
import { entry as askDock } from "./ask-dock.tsx"
import { entry as dockCard } from "./dock-card.tsx"
import { entry as planCard } from "./plan-card.tsx"
import { entry as scriptCard } from "./script-card.tsx"
import { entry as commentCallCard } from "./comment-call-card.tsx"
import { entry as teamAgentsSettings } from "./team-agents-settings.tsx"
import { entry as voiceCallOverlay } from "./voice-call-overlay.tsx"
import { entry as voiceCall } from "./voice-call.tsx"
import { entry as computerUseBanner } from "./computer-use-banner.tsx"
import { entry as voiceSettings } from "./voice-settings.tsx"
import { entry as meetingConsent } from "./meeting-consent.tsx"
import { entry as feedbackStates } from "./feedback-states.tsx"
import { entry as agentHome } from "./agent-home.tsx"
import { entry as workTabsStrip } from "./work-tabs-strip.tsx"
import { entry as searchList } from "./search-list.tsx"
import { entry as menuSubmenu } from "./menu-submenu.tsx"
import { entry as gettingStarted } from "./getting-started.tsx"
import { entry as chatList } from "./chat-list.tsx"
import { entry as phoneTabBar } from "./phone-tab-bar.tsx"
import { entry as phoneBoardMenu } from "./phone-board-menu.tsx"
import { entry as phonePageHeader } from "./phone-page-header.tsx"
import { entry as chatTranscript } from "./chat-transcript.tsx"
import { entry as chatMenus } from "./chat-menus.tsx"
import { entry as storyRing } from "./story-ring.tsx"
import { entry as storyViewer } from "./story-viewer.tsx"
import { entry as chatSelection } from "./chat-selection.tsx"
import { entry as trace } from "./trace.tsx"
import { entry as codeBlock } from "./code-block.tsx"
import type { StyleguideEntry } from "./types.ts"

export type { StyleguideEntry } from "./types.ts"

export const ENTRIES: readonly StyleguideEntry[] = [
  pickerBoard,
  pickerIssue,
  pickerAccount,
  pickerDevice,
  pickerAssignee,
  pickerIcon,
  pickerStatus,
  pickerPriority,
  pickerLabel,
  pickerMcp,
  pickerRepository,
  scopePicker,
  schedulePicker,
  searchList,
  menuSubmenu,
  toast,
  jumpToBottom,
  stripChip,
  sessionXref,
  mentionPill,
  feedbackStates,
  trace,
  codeBlock,
  composerDialog,
  issueContextMenu,
  sessionRow,
  prRow,
  prGraphBadge,
  deviceSettings,
  workScreen,
  stackMergeChoiceDialog,
  draftLeaveDialog,
  readinessChecklist,
  deviceReadiness,
  deviceAttention,
  resultsGuide,
  mcpAppViews,
  runStatusRow,
  sessionThread,
  composerAgentRow,
  workCard,
  threadParts,
  askDock,
  dockCard,
  planCard,
  scriptCard,
  commentCallCard,
  teamAgentsSettings,
  voiceCallOverlay,
  voiceCall,
  computerUseBanner,
  voiceSettings,
  meetingConsent,
  agentHome,
  workTabsStrip,
  gettingStarted,
  chatList,
  phoneTabBar,
  phoneBoardMenu,
  phonePageHeader,
  chatTranscript,
  chatMenus,
  chatSelection,
  storyRing,
  storyViewer,
]

export function entryById(id: string): StyleguideEntry | undefined {
  return ENTRIES.find((entry) => entry.id === id)
}
