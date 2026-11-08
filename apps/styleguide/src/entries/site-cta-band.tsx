import { CTA_BOX, CTA_CURVES } from "../../../site/src/components/landing/cta-curves.ts"

import { siteScope } from "./site-parts.tsx"
import type { StyleguideEntry } from "./types.ts"

// The site's closing band (exponential.at, CtaBand.astro): the site's own
// markup, curves (landing/cta-curves.ts) and stylesheet (parts.css). Web
// only: the site is a web page.

const FIT = `.sg-site .exp-cta { padding-block: 72px; } .sg-site .exp-cta::before { display: none; } .sg-site .exp-cta-motif { inset: 0; width: 100%; translate: none; } .sg-site .exp-cta h2 { font-size: 44px; }`

export const entry: StyleguideEntry = {
  id: `site-cta-band`,
  section: `special`,
  owner: `EXP-1740`,
  title: `Site closing band`,
  blurb: `The band above the footer of every exponential.at page outside the docs: one line at up to 88 px (600, -0.05em), the two ways in, over the exponential curve motif. Eight hairlines y = e^(kx) at 9 % white fan from the bottom-left corner up the end edge and draw in as the band scrolls into view (a CSS view timeline; still under reduced motion). The site's other motion lives in the same file: the hero headline, tagline and actions rise 0.6em in 700 ms, 80 ms apart, and framed visuals rise 2rem from 0.985 scale as they enter the viewport.`,
  status: {
    web: { state: `ok`, symbol: `CtaBand.astro + parts.css .exp-cta`, file: `apps/site/src/components/landing/CtaBand.astro`, note: `curves: apps/site/src/components/landing/cta-curves.ts` },
    desktop: { state: `n/a`, note: `The site is a web page.` },
    ios: { state: `n/a`, note: `The site is a web page.` },
    android: { state: `n/a`, note: `The site is a web page.` },
  },
  island: () =>
    siteScope(
      <div className="sg-site">
        <div className="exp-cta">
          <svg className="exp-cta-motif" viewBox={`0 0 ${CTA_BOX.width} ${CTA_BOX.height}`} preserveAspectRatio="none" aria-hidden="true">
            {CTA_CURVES.map((d) => (
              <path key={d} d={d} pathLength={1} />
            ))}
          </svg>
          <h2>Build your first app.</h2>
          <div className="exp-actions">
            <a className="sg-site-button primary" href="https://app.exponential.at/auth/login">
              Get started free →
            </a>
            <a className="sg-site-button" href="https://exponential.at/download/">
              Download
            </a>
          </div>
        </div>
      </div>,
      FIT
    ),
}
