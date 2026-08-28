// EXP-375 — human determinations for packages whose metadata cannot be
// resolved mechanically.
//
// The collectors are deliberately dumb: they read what a package declares and
// reproduce what it ships, and they refuse to guess. A handful of packages
// declare nothing at all, or declare a name that is not an SPDX id. Guessing
// inside a collector would hide the guess; recording it here makes it a dated,
// reviewable determination sitting next to `docs/third-party-licences.md`.
//
// The generator applies these AFTER loading the inventories, and FAILS if an
// override matches nothing — so an entry cannot quietly go stale when the
// dependency it was written for is removed or starts declaring a licence.

export interface LicenceOverride {
  /** Inventory the override applies to. */
  scope: `desktop` | `web` | `marketing` | `ios` | `android`
  /** Package name, or a `*`-suffixed prefix for a platform-variant family. */
  name: string
  /** What the notice should print as the declared terms. */
  declared: string
  /**
   * Elected SPDX ids. EMPTY means "not open source" — the generator routes the
   * component into the commercially-licensed section instead of the aggregate.
   */
  licenses: string[]
  /**
   * Reproduce the canonical SPDX body for these ids when the package ships no
   * licence file of its own. Omit to keep whatever the collector already found.
   */
  useTemplate?: boolean
  /**
   * Copyright lines to attach when the package ships none we can read. Needed
   * for platform-gated packages, whose licence file only exists on the one host
   * that installs that variant and therefore may never be read from disk.
   */
  copyright?: string[]
  /** Why, and on what evidence. Printed nowhere; read by the next maintainer. */
  reason: string
}

export const LICENCE_OVERRIDES: LicenceOverride[] = [
  {
    scope: `web`,
    name: `@better-fetch/fetch`,
    declared: `MIT`,
    licenses: [`MIT`],
    reason:
      `Checked 2026-07-31: package.json carries no \`license\` field, but the ` +
      `package ships a LICENSE file whose body is the MIT licence, ` +
      `"Copyright (c) Bereket Engida". The shipped file is reproduced; this ` +
      `override only supplies the id the manifest omits.`,
  },
  {
    scope: `web`,
    name: `creem`,
    declared: `MIT`,
    licenses: [`MIT`],
    useTemplate: true,
    reason:
      `Checked 2026-07-31: this is the Speakeasy-generated Creem SDK. It ships ` +
      `neither a \`license\` field nor a licence file, but its README carries ` +
      `an MIT badge and its upstream repository ` +
      `(github.com/armitage-labs/creem, the packages/creem-sdk directory) is ` +
      `MIT. Canonical MIT text is reproduced because the package ships none.`,
  },
  {
    scope: `web`,
    name: `fsevents`,
    declared: `MIT`,
    licenses: [`MIT`],
    copyright: [
      `Copyright (C) 2010-2020 by Philipp Dunkel, Ben Noordhuis, Elan Shanker, Paul Miller`,
    ],
    reason:
      `Checked 2026-07-31: fsevents is macOS-only (\`os: ["darwin"]\`), so its ` +
      `licence file is only on disk for a macOS collector run and the ` +
      `collector deliberately does not read it — otherwise the inventory would ` +
      `differ between a macOS and an ubuntu runner. It declares MIT, so the ` +
      `canonical body is reproduced; its copyright line is restored here ` +
      `because that, not the permission text, is what MIT requires us to carry. ` +
      `Every other gated family has a non-gated parent in the closure that ` +
      `carries the copyright already.`,
  },
  // Remotion is source-available, not open source. Its packages spell that
  // five different ways across their manifests — `SEE LICENSE IN LICENSE.md`
  // (whose target file none of them actually ships), `Remotion License`,
  // `Remotion License https://remotion.dev/license`, `UNLICENSED`, and nothing
  // at all on the prebuilt compositor binaries. All of them must read the same
  // in the notice, and all of them must stay OUT of the open-source aggregate
  // (empty `licenses`). See docs/third-party-licences.md for the dated
  // determination and the reproduced licence text.
  {
    scope: `marketing`,
    name: `remotion`,
    declared: `Remotion License (https://remotion.dev/license)`,
    licenses: [],
    reason:
      `Checked 2026-07-31: declares \`SEE LICENSE IN LICENSE.md\` but ships no ` +
      `such file. Source-available under the Remotion License, reproduced in ` +
      `the commercially-licensed section from docs/licences/remotion-LICENSE.txt.`,
  },
  {
    scope: `marketing`,
    name: `@remotion/*`,
    declared: `Remotion License (https://remotion.dev/license)`,
    licenses: [],
    reason:
      `Checked 2026-07-31: covers every @remotion/* package in the marketing ` +
      `closure — the ones declaring \`SEE LICENSE IN LICENSE.md\` without ` +
      `shipping it, the free-text "Remotion License" ones, \`UNLICENSED\` ` +
      `(@remotion/web-renderer) and the prebuilt @remotion/compositor-* ` +
      `binaries that declare nothing at all. All ship under the Remotion ` +
      `License as part of the same distribution.`,
  },
  {
    scope: `desktop`,
    name: `rio-vt`,
    declared: `MIT`,
    licenses: [`MIT`],
    useTemplate: true,
    copyright: [`Copyright (c) 2023-Present Raphael Amorim`],
    reason:
      `Checked 2026-08-28 (EXP-636 rio-vt migration): the crate tarball ships ` +
      `no LICENSE file, so the collector fell back to the bare MIT template ` +
      `with a placeholder copyright line. Upstream (github.com/raphamorim/rio, ` +
      `LICENSE) is MIT, "Copyright (c) 2023-Present Raphael Amorim"; the ` +
      `sibling rio-grapheme-width crate ships that exact line. MIT requires ` +
      `carrying the notice, so it is restored here.`,
  },
  {
    scope: `desktop`,
    name: `rio-graphics`,
    declared: `MIT`,
    licenses: [`MIT`],
    useTemplate: true,
    copyright: [`Copyright (c) 2023-Present Raphael Amorim`],
    reason:
      `Checked 2026-08-28 (EXP-636 rio-vt migration): the crate tarball ships ` +
      `no LICENSE file, so the collector fell back to the bare MIT template ` +
      `with a placeholder copyright line. Upstream (github.com/raphamorim/rio, ` +
      `LICENSE) is MIT, "Copyright (c) 2023-Present Raphael Amorim"; the ` +
      `sibling rio-grapheme-width crate ships that exact line. MIT requires ` +
      `carrying the notice, so it is restored here.`,
  },
  {
    scope: `desktop`,
    name: `rio-unicode`,
    declared: `MIT`,
    licenses: [`MIT`],
    useTemplate: true,
    copyright: [`Copyright (c) 2023-Present Raphael Amorim`],
    reason:
      `Checked 2026-08-28 (EXP-636 rio-vt migration): the crate tarball ships ` +
      `no LICENSE file, so the collector fell back to the bare MIT template ` +
      `with a placeholder copyright line. Upstream (github.com/raphamorim/rio, ` +
      `LICENSE) is MIT, "Copyright (c) 2023-Present Raphael Amorim"; the ` +
      `sibling rio-grapheme-width crate ships that exact line. MIT requires ` +
      `carrying the notice, so it is restored here.`,
  },
]

/** Exact name first, then `*` prefixes, longest prefix wins. */
export function findOverride(
  scope: LicenceOverride[`scope`],
  name: string
): LicenceOverride | undefined {
  const inScope = LICENCE_OVERRIDES.filter((o) => o.scope === scope)
  const exact = inScope.find((o) => o.name === name)
  if (exact) return exact
  return inScope
    .filter((o) => o.name.endsWith(`*`))
    .sort((a, b) => b.name.length - a.name.length)
    .find((o) => name.startsWith(o.name.slice(0, -1)))
}
