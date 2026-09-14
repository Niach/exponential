/* ─── Small lucide wrapper for the IDE recreation (14px default, product stroke) ─── */
import type { ComponentType, CSSProperties } from "react"
import {
  ArrowUpRight,
  Bell,
  Bot,
  ChevronsLeftRight,
  CircleSlash,
  Ellipsis,
  ExternalLink,
  GitBranch,
  Hash,
  Image as ImageGlyph,
  LifeBuoy,
  MessageSquareHeart,
  SquareKanban,
  Megaphone,
  PanelLeftClose,
  Paperclip,
  Smile,
  Sparkles,
  SquarePen,
  Trash2,
  Upload,
  BellOff,
  Bold,
  CalendarDays,
  CalendarSync,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  ChevronsUpDown,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleDot,
  CircleUser,
  CircleX,
  Code,
  File,
  FlaskConical,
  Folder,
  FolderOpen,
  GitMerge,
  GitPullRequest,
  Heading1,
  Heading2,
  Heading3,
  ImagePlus,
  Inbox,
  Italic,
  Link,
  List,
  ListChecks,
  ListFilter,
  ListOrdered,
  ListTodo,
  MessageSquare,
  Minus,
  Monitor,
  Package,
  Play,
  Plus,
  RefreshCw,
  RemoveFormatting,
  Search,
  Send,
  Settings,
  SignalHigh,
  SignalLow,
  SignalMedium,
  SquareTerminal,
  Strikethrough,
  Tag,
  TextQuote,
  Timer,
  TriangleAlert,
  User,
  UserPlus,
  X,
  Zap,
  Link2,
  MessageCircle,
  Terminal,
  Wrench,
  CircleArrowUp,
  CircleQuestionMark,
  CircleStop,
  Pin,
  RotateCcw,
  type LucideProps,
} from "lucide-react"

export type IdeIconProps = {
  size?: number
  className?: string
  style?: CSSProperties
}

export type IdeIcon = ComponentType<IdeIconProps>

const wrap = (Cmp: ComponentType<LucideProps>): IdeIcon =>
  function IdeWrappedIcon({ size = 14, className, style }: IdeIconProps) {
    return <Cmp size={size} strokeWidth={1.6} className={className} style={style} />
  }

/* Rail + chrome */
export const IcSearch = wrap(Search)
export const IcInbox = wrap(Inbox)
export const IcCircleUser = wrap(CircleUser)
export const IcListTodo = wrap(ListTodo)
export const IcFolder = wrap(Folder)
export const IcFolderOpen = wrap(FolderOpen)
export const IcFile = wrap(File)
export const IcGitMerge = wrap(GitMerge)
export const IcGitPullRequest = wrap(GitPullRequest)
export const IcUserPlus = wrap(UserPlus)
export const IcMessageSquare = wrap(MessageSquare)
export const IcCircleDot = wrap(CircleDot)
export const IcSettings = wrap(Settings)
export const IcChevsUpDown = wrap(ChevronsUpDown)
export const IcChevDown = wrap(ChevronDown)
export const IcChevUp = wrap(ChevronUp)
export const IcChevRight = wrap(ChevronRight)
export const IcChevLeft = wrap(ChevronLeft)
export const IcPlay = wrap(Play)
export const IcCheck = wrap(Check)
export const IcRefresh = wrap(RefreshCw)
export const IcSquareTerminal = wrap(SquareTerminal)
export const IcX = wrap(X)
export const IcPlus = wrap(Plus)
export const IcListFilter = wrap(ListFilter)

/* Status / priority */
export const IcTimer = wrap(Timer)
export const IcCircle = wrap(Circle)
export const IcCircleDashed = wrap(CircleDashed)
export const IcCircleCheck = wrap(CircleCheck)
export const IcCircleX = wrap(CircleX)
export const IcMinus = wrap(Minus)
export const IcAlert = wrap(TriangleAlert)
export const IcSigHigh = wrap(SignalHigh)
export const IcSigMed = wrap(SignalMedium)
export const IcSigLow = wrap(SignalLow)
export const IcCalDays = wrap(CalendarDays)
export const IcCalSync = wrap(CalendarSync)
export const IcUser = wrap(User)
export const IcTag = wrap(Tag)

/* Issue detail */
export const IcBell = wrap(Bell)
export const IcBellOff = wrap(BellOff)
export const IcSend = wrap(Send)
export const IcH1 = wrap(Heading1)
export const IcH2 = wrap(Heading2)
export const IcH3 = wrap(Heading3)
export const IcBold = wrap(Bold)
export const IcItalic = wrap(Italic)
export const IcStrike = wrap(Strikethrough)
export const IcCode = wrap(Code)
export const IcLink = wrap(Link)
export const IcQuote = wrap(TextQuote)
export const IcList = wrap(List)
export const IcListOrdered = wrap(ListOrdered)
export const IcListChecks = wrap(ListChecks)
export const IcClearFmt = wrap(RemoveFormatting)
export const IcImage = wrap(ImagePlus)

/* Rail / chrome concepts added for the EXP-471 pass */
export const IcBot = wrap(Bot)
export const IcLifeBuoy = wrap(LifeBuoy)
export const IcSparkles = wrap(Sparkles)
export const IcPanelLeftClose = wrap(PanelLeftClose)
/* nav-devices = monitor, nav-automations = zap (packages/icons/icons.json) */
export const IcMonitor = wrap(Monitor)
export const IcZap = wrap(Zap)
/* ui-undock = arrow-up-right — the dock's "Open in new window" */
export const IcArrowUpRight = wrap(ArrowUpRight)
/* Curated action glyphs the seeded team actions carry (boardIcon set). */
export const IcPackage = wrap(Package)
export const IcFlask = wrap(FlaskConical)
export const IcTrash = wrap(Trash2)
export const IcEllipsis = wrap(Ellipsis)
export const IcExternalLink = wrap(ExternalLink)
export const IcGitBranch = wrap(GitBranch)
export const IcMegaphone = wrap(Megaphone)
export const IcMessageHeart = wrap(MessageSquareHeart)
export const IcKanban = wrap(SquareKanban)
export const IcPaperclip = wrap(Paperclip)
export const IcSmile = wrap(Smile)
export const IcSquarePen = wrap(SquarePen)
export const IcCircleSlash = wrap(CircleSlash)
export const IcChevsLeftRight = wrap(ChevronsLeftRight)
export const IcUpload = wrap(Upload)

/* ─── The positional pie-clock glyphs for the `started` category
   (packages/icons/icons.json `custom`; EXP-314 parity across all clients).
   With the two builtin started statuses N=2 → [2/4, 3/4]. ─── */
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
        strokeWidth={1.6}
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
// EXP-723/742 rail + dock chrome and the relations card
export const IcMessageCircle = wrap(MessageCircle) // action-chat
export const IcLink2 = wrap(Link2) // relation-section
export const IcTerminal = wrap(Terminal) // session-shell
/* The session transcript (EXP-746/787): narration wears `coding-assistant`
   (sparkles, IcSparkles above), a tool row `coding-tool`, an answerable card
   `ui-help`, the composer `ui-submit` and the header's kill `coding-stop`. */
export const IcWrench = wrap(Wrench) // coding-tool
export const IcCircleQuestion = wrap(CircleQuestionMark) // ui-help
export const IcCircleArrowUp = wrap(CircleArrowUp) // ui-submit
export const IcCircleStop = wrap(CircleStop) // coding-stop
export const IcHash = wrap(Hash) // editor-issue-ref (the composer's issue picker)
export const IcImageConcept = wrap(ImageGlyph) // editor-image
export const IcPin = wrap(Pin) // tab pin (issue header)
export const IcRotateCcw = wrap(RotateCcw) // run-resume

/* ─── Agent BRAND marks (hand-maintained, apps/desktop/assets/icons): the
   REAL Claude mark in its brand orange (theme::CLAUDE_BRAND, EXP-877) and
   the Codex mark in currentColor. They lead the tab strip's agent groups
   and the launch pickers, never a list row (EXP-874). ─── */
const CLAUDE_PATH = `m19.6 66.5 19.7-11 .3-1-.3-.5h-1l-3.3-.2-11.2-.3L14 53l-9.5-.5-2.4-.5L0 49l.2-1.5 2-1.3 2.9.2 6.3.5 9.5.6 6.9.4L38 49.1h1.6l.2-.7-.5-.4-.4-.4L29 41l-10.6-7-5.6-4.1-3-2-1.5-2-.6-4.2 2.7-3 3.7.3.9.2 3.7 2.9 8 6.1L37 36l1.5 1.2.6-.4.1-.3-.7-1.1L33 25l-6-10.4-2.7-4.3-.7-2.6c-.3-1-.4-2-.4-3l3-4.2L28 0l4.2.6L33.8 2l2.6 6 4.1 9.3L47 29.9l2 3.8 1 3.4.3 1h.7v-.5l.5-7.2 1-8.7 1-11.2.3-3.2 1.6-3.8 3-2L61 2.6l2 2.9-.3 1.8-1.1 7.7L59 27.1l-1.5 8.2h.9l1-1.1 4.1-5.4 6.9-8.6 3-3.5L77 13l2.3-1.8h4.3l3.1 4.7-1.4 4.9-4.4 5.6-3.7 4.7-5.3 7.1-3.2 5.7.3.4h.7l12-2.6 6.4-1.1 7.6-1.3 3.5 1.6.4 1.6-1.4 3.4-8.2 2-9.6 2-14.3 3.3-.2.1.2.3 6.4.6 2.8.2h6.8l12.6 1 3.3 2 1.9 2.7-.3 2-5.1 2.6-6.8-1.6-16-3.8-5.4-1.3h-.8v.4l4.6 4.5 8.3 7.5L89 80.1l.5 2.4-1.3 2-1.4-.2-9.2-7-3.6-3-8-6.8h-.5v.7l1.8 2.7 9.8 14.7.5 4.5-.7 1.4-2.6 1-2.7-.6-5.8-8-6-9-4.7-8.2-.5.4-2.9 30.2-1.3 1.5-3 1.2-2.5-2-1.4-3 1.4-6.2 1.6-8 1.3-6.4 1.2-7.9.7-2.6v-.2H49L43 72l-9 12.3-7.2 7.6-1.7.7-3-1.5.3-2.8L24 86l10-12.8 6-7.9 4-4.6-.1-.5h-.3L17.2 77.4l-4.7.6-2-2 .2-3 1-1 8-5.5Z`

export const CLAUDE_BRAND = `#d97757`

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

const CODEX_PATH = `M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z`

export function IcCodex({ size = 14, className, style }: IdeIconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      className={className}
      style={style}
      aria-hidden
    >
      <path d={CODEX_PATH} />
    </svg>
  )
}
