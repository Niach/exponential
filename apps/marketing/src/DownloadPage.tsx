import { motion } from "motion/react"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { PLATFORMS } from "./components/DownloadSection"
import { LINKS } from "./lib/links"
import { IcArrow, IcGithub } from "./components/icons"
import {
  cardReveal,
  heroChild,
  heroStagger,
  staggerContainer,
  viewportOnce,
} from "./lib/animations"

export function DownloadPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="hero dl-hero">
          <motion.div
            className="shell hero-content"
            variants={heroStagger}
            initial="hidden"
            animate="visible"
          >
            <motion.h1 className="hero-title" variants={heroChild}>
              Get Exponential.
            </motion.h1>
            <motion.p className="hero-sub" variants={heroChild}>
              Native on every platform.
            </motion.p>
          </motion.div>
        </section>

        <section style={{ paddingTop: 0 }}>
          <div className="shell">
            <motion.div
              className="dl-grid"
              variants={staggerContainer}
              initial="hidden"
              whileInView="visible"
              viewport={viewportOnce}
            >
              {PLATFORMS.map((p) => {
                const Logo = p.logo
                return (
                  <motion.a
                    key={p.id}
                    className={`dl-card${p.soon ? ` is-soon` : ``}`}
                    href={p.href}
                    variants={cardReveal}
                  >
                    <span className="dl-card-icon">
                      <Logo size={26} />
                    </span>
                    <span className="dl-card-name">{p.name}</span>
                    <span className="dl-card-req">{p.requirement}</span>
                    <span className={`dl-card-btn${p.soon ? ` is-soon` : ``}`}>
                      {p.cta}
                      {!p.soon && <IcArrow size={12} />}
                    </span>
                  </motion.a>
                )
              })}
            </motion.div>

            <p className="dl-releases">
              <IcGithub size={14} />
              <span>
                Checksums and every past version live on{` `}
                <a href={LINKS.github.releases}>GitHub Releases</a>.
              </span>
            </p>
          </div>
        </section>
      </main>

      <SiteFooter />
    </>
  )
}
