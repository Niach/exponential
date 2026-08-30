/* ─── Lucide wrappers for the web-app recreation ───
   The web app renders raw lucide at its DEFAULT stroke (2) and sizes icons in
   rem (`size-4` = 1rem = 18.5px at the app's 1.15625rem root), so this set
   deliberately differs from ide/icons.tsx (1.6 stroke, 14px default). Glyph
   choices mirror packages/icons/icons.json — the concept name is in the
   comment beside every export. */
import type { ComponentType, CSSProperties } from "react"
import {
  AtSign,
  Bell,
  BellOff,
  Bot,
  CalendarDays,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  ChevronsUpDown,
  CircleCheck,
  CircleDashed,
  CircleDot,
  CircleUser,
  Code,
  ExternalLink,
  GitMerge,
  GitPullRequest,
  Image as ImageIcon,
  Inbox,
  LifeBuoy,
  Link2,
  ListFilter,
  Lock,
  Mail,
  Megaphone,
  MessageSquare,
  Minus,
  MonitorPlay,
  Paperclip,
  Plus,
  Search,
  Send,
  Settings,
  SignalHigh,
  SignalLow,
  SignalMedium,
  Smile,
  Sparkles,
  SquareKanban,
  SquarePen,
  StickyNote,
  Tag,
  Trash2,
  TriangleAlert,
  User,
  UserPlus,
  type LucideProps,
} from "lucide-react"

export type WebIconProps = {
  size?: number
  className?: string
  style?: CSSProperties
}

export type WebIcon = ComponentType<WebIconProps>

/* `size-4` — the app's default icon box (1rem at the 18.5px root). */
export const ICON_4 = 18.5
/* `size-3.5` */
export const ICON_35 = 16.1875
/* `size-3` */
export const ICON_3 = 13.875

const wrap = (Cmp: ComponentType<LucideProps>): WebIcon =>
  function WebWrappedIcon({ size = ICON_4, className, style }: WebIconProps) {
    return <Cmp size={size} className={className} style={style} />
  }

/* Sidebar nav (nav-*) */
export const IcSearch = wrap(Search) // nav-search
export const IcCompose = wrap(SquarePen) // nav-create-issue
export const IcTeamSwitcher = wrap(ChevronsUpDown) // nav-team-switcher
export const IcInbox = wrap(Inbox) // nav-inbox
export const IcReviews = wrap(GitPullRequest) // nav-reviews
export const IcAgents = wrap(Bot) // nav-agents
export const IcSupport = wrap(LifeBuoy) // nav-support
export const IcSparkles = wrap(Sparkles) // nav-getting-started
export const IcSettings = wrap(Settings) // nav-settings
export const IcPlus = wrap(Plus) // ui-add
export const IcFilter = wrap(ListFilter) // nav-filter

/* Board glyphs — a board with a repository renders `ui-repository`. */
export const IcCode = wrap(Code) // ui-repository
export const IcKanban = wrap(SquareKanban) // settings-boards
export const IcMegaphone = wrap(Megaphone) // ui-widget

/* Status / priority */
export const IcCircleDashed = wrap(CircleDashed) // status-backlog
export const IcCircleCheck = wrap(CircleCheck) // status-done
export const IcMinus = wrap(Minus) // priority-none
export const IcTriangleAlert = wrap(TriangleAlert) // priority-urgent
export const IcSignalHigh = wrap(SignalHigh) // priority-high
export const IcSignalMedium = wrap(SignalMedium) // priority-medium
export const IcSignalLow = wrap(SignalLow) // priority-low

/* Rows / issue detail */
export const IcChevRight = wrap(ChevronRight)
export const IcChevDown = wrap(ChevronDown)
export const IcChevUp = wrap(ChevronUp)
export const IcCalendar = wrap(CalendarDays) // ui-due-date
export const IcUser = wrap(User) // ui-avatar-placeholder
export const IcTag = wrap(Tag) // settings-labels
export const IcLink2 = wrap(Link2)
export const IcBell = wrap(Bell) // ui-subscribe
export const IcBellOff = wrap(BellOff) // ui-unsubscribe
export const IcTrash = wrap(Trash2) // ui-delete
export const IcSmile = wrap(Smile)
export const IcImage = wrap(ImageIcon)
export const IcPaperclip = wrap(Paperclip) // ui-attach
export const IcWatch = wrap(MonitorPlay)
export const IcCheck = wrap(Check) // ui-check

/* Inbox notification kinds */
export const IcAssigned = wrap(UserPlus) // ui-invite
export const IcComment = wrap(MessageSquare)
export const IcMention = wrap(AtSign)
export const IcMerged = wrap(GitMerge) // nav-source-control
export const IcStatusChanged = wrap(CircleDot)
export const IcAssignee = wrap(CircleUser) // ui-assignee

/* Support */
export const IcSupportOpen = wrap(CircleDot) // support-open
export const IcSupportResolved = wrap(CircleCheck) // support-resolved
export const IcLifeBuoy = wrap(LifeBuoy)
export const IcMail = wrap(Mail)
export const IcStickyNote = wrap(StickyNote)
export const IcSend = wrap(Send) // ui-send
export const IcExternalLink = wrap(ExternalLink) // ui-external-link
export const IcLock = wrap(Lock) // ui-private

