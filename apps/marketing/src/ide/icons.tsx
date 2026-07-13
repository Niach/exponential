/* ─── Small lucide wrapper for the IDE recreation (14px default, product stroke) ─── */
import type { ComponentType, CSSProperties } from "react"
import {
  Bell,
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
  Play,
  Plus,
  RefreshCw,
  RemoveFormatting,
  Rocket,
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
export const IcRocket = wrap(Rocket)
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
