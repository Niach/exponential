/**
 * EXP-1029 contract — every entry under the four sections, by fixed export
 * name. A leaf fills ITS file; nobody edits this list (EXP-1019 splices it
 * into the page when it moves the existing entries into the sections).
 */
import { entry as picker } from "./picker.tsx"
import { entry as pickerBoard } from "./picker-board.tsx"
import { entry as pickerIssue } from "./picker-issue.tsx"
import { entry as pickerAction } from "./picker-action.tsx"
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
import { entry as menu } from "./menu.tsx"
import { entry as toast } from "./toast.tsx"
import { entry as jumpToBottom } from "./jump-to-bottom.tsx"
import { entry as composerDialog } from "./composer-dialog.tsx"
import { entry as issueContextMenu } from "./issue-context-menu.tsx"
import { entry as sessionTree } from "./session-tree.tsx"
import { entry as prGraphBadge } from "./pr-graph-badge.tsx"
import { entry as deviceSettings } from "./device-settings.tsx"
import { entry as blockedStartDialog } from "./blocked-start-dialog.tsx"
import { entry as stackMergeChoiceDialog } from "./stack-merge-choice-dialog.tsx"
import { entry as draftLeaveDialog } from "./draft-leave-dialog.tsx"
import { entry as readinessChecklist } from "./readiness-checklist.tsx"
import { entry as deviceReadiness } from "./device-readiness.tsx"
import { entry as resultsGuide } from "./results-guide.tsx"
import { entry as mcpAppViews } from "./mcp-app-views.tsx"
import { entry as runStatusRow } from "./run-status-row.tsx"
import { entry as sessionThread } from "./session-thread.tsx"
import type { StyleguideEntry } from "./types.ts"

export type { StyleguideEntry } from "./types.ts"

export const ENTRIES: readonly StyleguideEntry[] = [
  picker,
  pickerBoard,
  pickerIssue,
  pickerAction,
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
  menu,
  toast,
  jumpToBottom,
  composerDialog,
  issueContextMenu,
  sessionTree,
  prGraphBadge,
  deviceSettings,
  blockedStartDialog,
  stackMergeChoiceDialog,
  draftLeaveDialog,
  readinessChecklist,
  deviceReadiness,
  resultsGuide,
  mcpAppViews,
  runStatusRow,
  sessionThread,
]

export function entryById(id: string): StyleguideEntry | undefined {
  return ENTRIES.find((entry) => entry.id === id)
}
