/* ─── The IDE recreation's glyphs ───
   The desktop app draws lucide SVGs (apps/desktop/assets/icons/*.svg, stroke
   width 2 on a 24 grid) picked by CONCEPT through packages/icons/icons.json
   `semantic`. Each export below names the concept it stands for, so a glyph
   here can be checked against that table. ─── */
import type { ComponentType, CSSProperties } from "react"
import {
  Bot,
  Rocket,
  Brain,
  Bug,
  CalendarDays,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronsUpDown,
  CircleArrowUp,
  CircleCheck,
  CircleDashed,
  CircleQuestionMark,
  CircleStop,
  CircleUser,
  CircleX,
  ClipboardList,
  Code,
  Computer,
  Copy,
  Ellipsis,
  File,
  FilePenLine,
  FileText,
  Folder,
  FolderOpen,
  Gauge,
  GitBranch,
  GitMerge,
  GitPullRequest,
  Hash,
  Hourglass,
  Image as ImageGlyph,
  Inbox,
  ListChecks,
  ListTree,
  MessageCircle,
  MessageSquare,
  Megaphone,
  Minus,
  Monitor,
  Package,
  Palette,
  Paperclip,
  Pencil,
  Pin,
  Play,
  Plug,
  Plus,
  RefreshCw,
  Search,
  Settings,
  SignalHigh,
  SignalLow,
  SignalMedium,
  Smile,
  Sparkles,
  SquareKanban,
  SquarePen,
  SquareTerminal,
  Tag,
  Terminal,
  Trash2,
  TriangleAlert,
  Upload,
  User,
  UserPlus,
  X,
  Zap,
  type LucideProps,
} from "lucide-react"

export type IdeIconProps = {
  size?: number
  className?: string
  style?: CSSProperties
}

export type IdeIcon = ComponentType<IdeIconProps>

/* The product SVGs carry stroke-width 2 at 24 units. */
const wrap = (Cmp: ComponentType<LucideProps>): IdeIcon =>
  function IdeWrappedIcon({ size = 14, className, style }: IdeIconProps) {
    return <Cmp size={size} strokeWidth={2} className={className} style={style} aria-hidden />
  }

/* nav-* / rail */
export const IcAgent = wrap(MessageCircle) // action-chat (the rail's Agent)
export const IcInbox = wrap(Inbox) // nav-inbox
export const IcThreads = wrap(ListTree) // nav-threads
export const IcMonitor = wrap(Monitor) // nav-devices, ui-device
export const IcDrafts = wrap(FilePenLine) // nav-drafts
export const IcSearch = wrap(Search) // nav-search
export const IcCreateIssue = wrap(SquarePen) // nav-create-issue
export const IcChevsUpDown = wrap(ChevronsUpDown) // nav-team-switcher
export const IcSettings = wrap(Settings) // nav-settings
export const IcComputer = wrap(Computer) // nav-computer
export const IcGettingStarted = wrap(Sparkles) // nav-getting-started
export const IcFiles = wrap(Folder) // nav-files, ui-folder
export const IcFolderOpen = wrap(FolderOpen) // ui-folder-open
export const IcFile = wrap(File) // ui-file
export const IcSourceControl = wrap(GitMerge) // nav-source-control
export const IcTerminal = wrap(SquareTerminal) // nav-terminal
export const IcPlus = wrap(Plus) // ui-add
export const IcX = wrap(X) // ui-close
export const IcCheck = wrap(Check) // ui-check
export const IcChevDown = wrap(ChevronDown) // ui-chevron-down
export const IcChevRight = wrap(ChevronRight) // ui-chevron-right
export const IcRefresh = wrap(RefreshCw) // ui-refresh
export const IcBranch = wrap(GitBranch) // ui-branch
export const IcUpload = wrap(Upload) // sc-push
export const IcTrash = wrap(Trash2) // ui-delete
export const IcEllipsis = wrap(Ellipsis) // ui-more
export const IcPin = wrap(Pin) // ui-pin
export const IcPlay = wrap(Play) // action-run (Start)
export const IcMarkRead = wrap(ListChecks) // notification-mark-read

/* status / priority (status-* / priority-*) */
export const IcCircleDashed = wrap(CircleDashed)
export const IcCircleCheck = wrap(CircleCheck)
export const IcCircleX = wrap(CircleX) // status-cancelled, coding-stop, trace-failed
export const IcCopy = wrap(Copy) // status-duplicate
export const IcMinus = wrap(Minus)
export const IcAlert = wrap(TriangleAlert) // priority-urgent, coding-needs-input
export const IcSigHigh = wrap(SignalHigh)
export const IcSigMed = wrap(SignalMedium)
export const IcSigLow = wrap(SignalLow)

/* issue detail */
export const IcCalDays = wrap(CalendarDays) // ui-due-date
export const IcUser = wrap(User) // ui-avatar-placeholder
export const IcAssignee = wrap(CircleUser) // ui-assignee
export const IcTag = wrap(Tag) // settings-labels
export const IcSmile = wrap(Smile) // editor-emoji
export const IcImage = wrap(ImageGlyph) // editor-image
export const IcPaperclip = wrap(Paperclip) // ui-attach

/* board glyphs (boardIcon set) */
export const IcCode = wrap(Code) // ui-repository / guide-changes
export const IcKanban = wrap(SquareKanban)
export const IcMegaphone = wrap(Megaphone)

/* agent glyphs (an agent row wears its own pickable icon) */
export const IcBug = wrap(Bug)
export const IcPalette = wrap(Palette)
export const IcPackage = wrap(Package)
export const IcRocket = wrap(Rocket) // an app glyph (pickable rocket)

/* inbox (notification-*) */
export const IcUserPlus = wrap(UserPlus) // notification-issue-assigned
export const IcComment = wrap(MessageSquare) // notification-issue-comment
export const IcPrOpened = wrap(GitPullRequest) // notification-pr-opened

/* session: Trace (trace-*), cards, composer */
export const IcTraceReasoning = wrap(Brain) // trace-reasoning
export const IcTraceTerminal = wrap(Terminal) // trace-terminal
export const IcTraceRead = wrap(FileText) // trace-read
export const IcTraceEdit = wrap(Pencil) // trace-edit
export const IcTraceSubagent = wrap(Bot) // trace-subagent
export const IcAssistant = wrap(Sparkles) // coding-assistant
export const IcQuestion = wrap(CircleQuestionMark) // ui-help
export const IcPlan = wrap(ClipboardList) // coding-plan
export const IcSubmit = wrap(CircleArrowUp) // ui-submit
export const IcStop = wrap(CircleStop) // ui-stop (the composer's Stop)
export const IcHourglass = wrap(Hourglass) // notification-session-blocked
export const IcIssueRef = wrap(Hash) // editor-issue-ref (Implement issue)
export const IcAgentsGlyph = wrap(Bot) // settings-agents (Message an agent)
export const IcEffort = wrap(Gauge) // ui-estimate (Effort)
export const IcUltracode = wrap(Zap) // action-default (Ultracode)
export const IcMcp = wrap(Plug) // ui-mcp

/* ─── The pie clocks of the `started` category (icons.json `custom`,
   EXP-314): with the two builtin started statuses N=2 → [2/4, 3/4]. ─── */
const clock = (d: string): IdeIcon =>
  function IdeClockIcon({ size = 14, className, style }: IdeIconProps) {
    return (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
        style={style}
        aria-hidden
      >
        <circle cx="12" cy="12" r="10" />
        <path d={d} fill="currentColor" stroke="none" />
      </svg>
    )
  }

export const IcProgress24 = clock(`M12 12 L12 6 A6 6 0 0 1 12 18 Z`)
export const IcProgress34 = clock(`M12 12 L12 6 A6 6 0 1 1 6 12 Z`)

/* ─── The Claude brand mark (apps/desktop/assets/icons/claude.svg, its own
   fill hsl(14.8 63.1% 59.6%)) — the run mark of every Claude session row,
   tab and chip. ─── */
const CLAUDE_PATH = `m19.6 66.5 19.7-11 .3-1-.3-.5h-1l-3.3-.2-11.2-.3L14 53l-9.5-.5-2.4-.5L0 49l.2-1.5 2-1.3 2.9.2 6.3.5 9.5.6 6.9.4L38 49.1h1.6l.2-.7-.5-.4-.4-.4L29 41l-10.6-7-5.6-4.1-3-2-1.5-2-.6-4.2 2.7-3 3.7.3.9.2 3.7 2.9 8 6.1L37 36l1.5 1.2.6-.4.1-.3-.7-1.1L33 25l-6-10.4-2.7-4.3-.7-2.6c-.3-1-.4-2-.4-3l3-4.2L28 0l4.2.6L33.8 2l2.6 6 4.1 9.3L47 29.9l2 3.8 1 3.4.3 1h.7v-.5l.5-7.2 1-8.7 1-11.2.3-3.2 1.6-3.8 3-2L61 2.6l2 2.9-.3 1.8-1.1 7.7L59 27.1l-1.5 8.2h.9l1-1.1 4.1-5.4 6.9-8.6 3-3.5L77 13l2.3-1.8h4.3l3.1 4.7-1.4 4.9-4.4 5.6-3.7 4.7-5.3 7.1-3.2 5.7.3.4h.7l12-2.6 6.4-1.1 7.6-1.3 3.5 1.6.4 1.6-1.4 3.4-8.2 2-9.6 2-14.3 3.3-.2.1.2.3 6.4.6 2.8.2h6.8l12.6 1 3.3 2 1.9 2.7-.3 2-5.1 2.6-6.8-1.6-16-3.8-5.4-1.3h-.8v.4l4.6 4.5 8.3 7.5L89 80.1l.5 2.4-1.3 2-1.4-.2-9.2-7-3.6-3-8-6.8h-.5v.7l1.8 2.7 9.8 14.7.5 4.5-.7 1.4-2.6 1-2.7-.6-5.8-8-6-9-4.7-8.2-.5.4-2.9 30.2-1.3 1.5-3 1.2-2.5-2-1.4-3 1.4-6.2 1.6-8 1.3-6.4 1.2-7.9.7-2.6v-.2H49L43 72l-9 12.3-7.2 7.6-1.7.7-3-1.5.3-2.8L24 86l10-12.8 6-7.9 4-4.6-.1-.5h-.3L17.2 77.4l-4.7.6-2-2 .2-3 1-1 8-5.5Z`

export const CLAUDE_BRAND = `hsl(14.8 63.1% 59.6%)`

export function IcClaude({ size = 14, className, style }: IdeIconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      fill={CLAUDE_BRAND}
      className={className}
      style={style}
      aria-hidden
    >
      <path d={CLAUDE_PATH} />
    </svg>
  )
}

/* ─── The Exponential mark (apps/desktop/assets/icons/logo.svg): the disc
   cut by three arcs, painted in currentColor — Home's dawn. ─── */
export function IcLogo({ size = 14, className, style }: IdeIconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
      style={style}
      aria-hidden
    >
      <defs>
        <clipPath id="ide-logo-clip">
          <circle cx="50" cy="50" r="50" />
        </clipPath>
        <mask id="ide-logo-mask">
          <rect width="100" height="100" fill="white" />
          <g clipPath="url(#ide-logo-clip)">
            <path d="M -5.87 62.01 C 39.09 65.44 48.72 28.71 49.03 -6.21" stroke="black" strokeWidth="3.5" fill="none" />
            <path d="M -5.07 86.00 C 53.78 84.42 71.13 37.29 73.00 -5.09" stroke="black" strokeWidth="3.5" fill="none" />
            <path d="M -4.27 109.99 C 68.46 103.40 93.55 45.86 96.98 -3.98" stroke="black" strokeWidth="3.5" fill="none" />
          </g>
        </mask>
      </defs>
      <circle cx="50" cy="50" r="50" fill="currentColor" mask="url(#ide-logo-mask)" />
    </svg>
  )
}
