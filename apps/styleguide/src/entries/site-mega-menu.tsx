import { MENUS } from "../../../site/src/components/landing/nav.ts"

import { siteScope } from "./site-parts.tsx"
import type { StyleguideEntry } from "./types.ts"

// The site header's two mega menus (exponential.at: Product, SDK), drawn
// open with the site's own markup, data (landing/nav.ts) and stylesheet
// (parts.css). Web only: the site is a web page.

const OPEN = `.sg-site .exp-mega-panel { position: static; visibility: visible; opacity: 1; transform: none; padding: 0; } .sg-site .sg-site-menus { display: flex; flex-wrap: wrap; gap: 16px; } .sg-site .exp-mega-body { grid-template-columns: minmax(0, 1fr); max-width: 26rem; }`

export const entry: StyleguideEntry = {
  id: `site-mega-menu`,
  section: `special`,
  owner: `EXP-1740`,
  title: `Site mega menu`,
  blurb: `exponential.at's header menus, Product and SDK: a panel hangs from the nav's end edge on hover or keyboard focus (CSS only, the CSP stays hash-only), fading and dropping 4 px in 200 ms on the site ease. Two columns of items, each an icon and a name, beside one featured picture with its name. Below 72rem the nav folds into a full-screen menu with the same groups.`,
  status: {
    web: { state: `ok`, symbol: `Header.astro + parts.css .exp-mega`, file: `apps/site/src/components/Header.astro`, note: `data: apps/site/src/components/landing/nav.ts` },
    desktop: { state: `n/a`, note: `The site is a web page.` },
    ios: { state: `n/a`, note: `The site is a web page.` },
    android: { state: `n/a`, note: `The site is a web page.` },
  },
  island: () =>
    siteScope(
      <div className="sg-site sg-site-menus">
        {MENUS.map((menu) => (
          <div className="exp-mega-panel" key={menu.label}>
            <div className="exp-mega-body">
              <ul className="exp-mega-items">
                {menu.items.map((item) => (
                  <li key={item.href}>
                    <a href={item.href}>
                      {item.icon && <item.icon size={18} strokeWidth={1.6} aria-hidden="true" />}
                      {item.label}
                    </a>
                  </li>
                ))}
              </ul>
              <a className="exp-mega-featured" href={menu.featured.href}>
                <img src={`https://exponential.at${menu.featured.src}`} alt={menu.featured.alt} />
                <span>{menu.featured.label} →</span>
              </a>
            </div>
          </div>
        ))}
      </div>,
      OPEN
    ),
}
