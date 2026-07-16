/* ─── Extra lucide wrappers for the web-app recreation ───
   Same 14px/1.6-stroke product defaults as ide/icons.tsx; the IDE set is
   reused directly, these are the web-only additions. */
import type { ComponentType } from "react"
import {
  Bot,
  Code2,
  ExternalLink,
  Globe,
  LifeBuoy,
  Link2,
  Lock,
  Mail,
  Megaphone,
  MoreHorizontal,
  Sparkles,
  SquareKanban,
  StickyNote,
  type LucideProps,
} from "lucide-react"
import type { IdeIcon, IdeIconProps } from "../ide/icons"

const wrap = (Cmp: ComponentType<LucideProps>): IdeIcon =>
  function WebWrappedIcon({ size = 14, className, style }: IdeIconProps) {
    return <Cmp size={size} strokeWidth={1.6} className={className} style={style} />
  }

/* Sidebar */
export const IcBot = wrap(Bot)
export const IcLifeBuoy = wrap(LifeBuoy)
export const IcGlobe = wrap(Globe)
export const IcCode2 = wrap(Code2)
export const IcKanban = wrap(SquareKanban)
export const IcMegaphone = wrap(Megaphone)
export const IcSparkles = wrap(Sparkles)

/* Issue detail */
export const IcMore = wrap(MoreHorizontal)
export const IcLink2 = wrap(Link2)

/* Support inbox */
export const IcMail = wrap(Mail)
export const IcStickyNote = wrap(StickyNote)
export const IcLock = wrap(Lock)
export const IcExternalLink = wrap(ExternalLink)
