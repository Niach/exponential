import { motion } from "motion/react"
import { Laptop, Smartphone, TabletSmartphone, Terminal } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import { LINKS } from "../lib/links"

const targets = [
  {
    platform: `macOS`,
    title: `Exponential for Mac`,
    desc: `Native Swift app with the embedded ghostty terminal — watch your agent work, live.`,
    cta: `download .dmg`,
    href: LINKS.downloads.macos,
    icon: <Laptop size={20} strokeWidth={1.8} />,
  },
  {
    platform: `Linux`,
    title: `Exponential for Linux`,
    desc: `Zig + GTK4, libadwaita-native. The same local agent runtime on your distro.`,
    cta: `download for linux`,
    href: LINKS.downloads.linux,
    icon: <Terminal size={20} strokeWidth={1.8} />,
  },
  {
    platform: `iOS`,
    title: `Exponential for iPhone`,
    desc: `SwiftUI, offline-first, push notifications. Approve agent plans from anywhere.`,
    cta: `get — App Store`,
    href: LINKS.downloads.ios,
    icon: <Smartphone size={20} strokeWidth={1.8} />,
  },
  {
    platform: `Android`,
    title: `Exponential for Android`,
    desc: `Jetpack Compose, real-time sync, the full markdown editor in your pocket.`,
    cta: `get — Google Play`,
    href: LINKS.downloads.android,
    icon: <TabletSmartphone size={20} strokeWidth={1.8} />,
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
          className="dl-card"
          href={t.href}
          variants={cardReveal}
        >
          <span className="dl-card-icon">{t.icon}</span>
          <span className="dl-card-platform">{t.platform}</span>
          <h3>{t.title}</h3>
          <p>{t.desc}</p>
          <span className="dl-cta">{t.cta}</span>
        </motion.a>
      ))}
    </motion.div>
  )
}
