import { motion } from "motion/react"
import { Apple, Monitor, Smartphone, TabletSmartphone, Terminal } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import { LINKS } from "../lib/links"

type Target = {
  platform: string
  title: string
  desc: string
  cta: string
  href: string
  icon: React.ReactNode
  soon?: boolean
}

const targets: Target[] = [
  {
    platform: `macOS`,
    title: `Exponential for Mac`,
    desc: `The Rust + gpui desktop IDE. Start coding on any issue, watch Claude work in the embedded terminal, review the diff, and ship the PR — all from one native window.`,
    cta: `download .dmg`,
    href: LINKS.downloads.macos,
    icon: <Apple size={20} strokeWidth={1.8} />,
  },
  {
    platform: `Linux`,
    title: `Exponential for Linux`,
    desc: `The same Rust + gpui IDE, packaged as a self-contained AppImage. Clone, code, and open pull requests on your distro — no runtime to install.`,
    cta: `download AppImage`,
    href: LINKS.downloads.linux,
    icon: <Terminal size={20} strokeWidth={1.8} />,
  },
  {
    platform: `Windows`,
    title: `Exponential for Windows`,
    desc: `The same Rust + gpui IDE on Windows. Unzip and run — the app registers itself for sign-in links on first launch.`,
    cta: `download .zip`,
    href: LINKS.downloads.windows,
    icon: <Monitor size={20} strokeWidth={1.8} />,
  },
  {
    platform: `iOS`,
    title: `Exponential for iPhone`,
    desc: `SwiftUI, offline-first, real-time sync. Triage issues, get push notifications, and steer a live coding session from your phone.`,
    cta: `App Store — coming soon`,
    href: LINKS.downloads.ios,
    icon: <Smartphone size={20} strokeWidth={1.8} />,
    soon: true,
  },
  {
    platform: `Android`,
    title: `Exponential for Android`,
    desc: `Jetpack Compose, real-time sync, the full markdown editor in your pocket — plus steer-from-phone for sessions running on your desktop.`,
    cta: `Google Play — coming soon`,
    href: LINKS.downloads.android,
    icon: <TabletSmartphone size={20} strokeWidth={1.8} />,
    soon: true,
  },
]

export function DownloadSection() {
  return (
    <motion.div
      className="dl-grid"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={viewportOnce}
    >
      {targets.map((t) => (
        <motion.a
          key={t.platform}
          className={`dl-card${t.soon ? ` is-soon` : ``}`}
          href={t.href}
          variants={cardReveal}
        >
          <span className="dl-card-icon">{t.icon}</span>
          <span className="dl-card-platform">
            {t.platform}
            {t.soon && <span className="dl-soon">soon</span>}
          </span>
          <h3>{t.title}</h3>
          <p>{t.desc}</p>
          <span className="dl-cta">{t.cta}</span>
        </motion.a>
      ))}
    </motion.div>
  )
}
