// EXP-375 — the hand-maintained half of the notices.
//
// The four collectors read dependency graphs. Everything below is something no
// dependency graph knows about: font binaries vendored into `public/`, icon
// geometry copied by a code generator, trademarks that must NOT sit under a
// licence heading, source-availability obligations that attribution alone does
// not discharge, and vendored source whose upstream is not a package at all.
//
// Licence BODIES are never written here — every `reproduce` entry names a file
// that already exists in the repository (or was fetched verbatim into
// `texts/`), and the generator inlines it. That way there is exactly one copy
// of each licence text and no opportunity to paraphrase one.
//
// Dated determinations that need a paper trail live in
// `docs/third-party-licences.md`; this file is the machine-readable projection
// of them.

export type Client = `desktop` | `web` | `marketing` | `ios` | `android`

export const CLIENTS: Client[] = [
  `android`,
  `desktop`,
  `ios`,
  `marketing`,
  `web`,
]

/** A repo-relative licence file to inline verbatim under a heading. */
export interface Reproduction {
  label: string
  path: string
}

export interface CuratedEntry {
  title: string
  clients: Client[]
  /** Prose paragraphs. Wrapped by the renderer; write them unwrapped. */
  body: string[]
  reproduce?: Reproduction[]
}

// ---------------------------------------------------------------------------
// Bundled fonts — OFL 1.1
// ---------------------------------------------------------------------------
//
// OFL §2 requires the licence to accompany the Font Software wherever it is
// distributed, which is why these bodies are reproduced in full rather than
// linked. Checked 2026-07-31: none of Inter, JetBrains Mono or Geist declares a
// Reserved Font Name in its copyright line, so OFL §3 (the rename obligation)
// does not bite and we may ship the families under their own names.
//
// The web app is deliberately absent: it loads Inter from Google Fonts at
// runtime and distributes no font binary of its own, so it has no OFL
// obligation to discharge.

export const FONTS: CuratedEntry[] = [
  {
    title: `Inter`,
    clients: [`desktop`, `marketing`],
    body: [
      `Copyright 2016 The Inter Project Authors (https://github.com/rsms/inter), licensed under the SIL Open Font License 1.1. The desktop application embeds the TTFs in its binary; the marketing site self-hosts woff2 subsets.`,
    ],
    reproduce: [
      { label: `SIL Open Font License 1.1 — Inter`, path: `apps/desktop/assets/fonts/LICENSE.txt` },
    ],
  },
  {
    title: `JetBrains Mono`,
    clients: [`desktop`, `marketing`],
    body: [
      `Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono), licensed under the SIL Open Font License 1.1.`,
    ],
    reproduce: [
      {
        label: `SIL Open Font License 1.1 — JetBrains Mono`,
        path: `apps/desktop/assets/fonts/JetBrainsMono-OFL.txt`,
      },
    ],
  },
  {
    title: `Geist and Geist Mono`,
    clients: [`marketing`],
    body: [
      `Copyright 2024 The Geist Project Authors (https://github.com/vercel/geist-font), licensed under the SIL Open Font License 1.1. Self-hosted as latin / latin-ext woff2 subsets.`,
    ],
    reproduce: [
      {
        label: `SIL Open Font License 1.1 — Geist`,
        path: `packages/licenses/texts/fonts/Geist-OFL.txt`,
      },
    ],
  },
]

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

export const ICONS: CuratedEntry[] = [
  {
    title: `Lucide`,
    clients: CLIENTS,
    body: [
      `Every client ships Lucide icon geometry. It is not consumed as a package on the native clients: packages/icons reads the path data out of lucide-react and emits it into per-platform sources — SVG assets, Compose ImageVectors, an Xcode asset catalogue and a Rust registry — so the same paths are reproduced in all four builds.`,
      `The desktop application additionally reaches gpui-component's own bundled set of 99 Lucide-derived SVGs through the asset-source fallback in apps/desktop/crates/app/src/assets.rs.`,
    ],
    reproduce: [
      { label: `ISC License — Lucide`, path: `apps/desktop/assets/icons/LICENSE.txt` },
    ],
  },
]

// ---------------------------------------------------------------------------
// Vendored source
// ---------------------------------------------------------------------------
//
// Neither of these is a package in any dependency graph. gpui-markdown-editor
// in particular is `publish = false`, so no cargo tool will ever emit it — this
// entry is the only route by which Velotype / manyougz gets attributed at all.

export const VENDORED: CuratedEntry[] = [
  {
    title: `Vendored source in the desktop application`,
    clients: [`desktop`],
    body: [
      `Two crates in this repository contain third-party source that was copied in and modified rather than depended on. Both are Apache-2.0, and both carry the statement of changes that Apache-2.0 section 4(b) requires. Their NOTICE files are reproduced in full below.`,
    ],
    reproduce: [
      {
        label: `apps/desktop/crates/ui/NOTICE`,
        path: `apps/desktop/crates/ui/NOTICE`,
      },
      {
        label: `apps/desktop/crates/gpui-markdown-editor/NOTICE`,
        path: `apps/desktop/crates/gpui-markdown-editor/NOTICE`,
      },
    ],
  },
]

// ---------------------------------------------------------------------------
// Trademarks — never under a licence heading
// ---------------------------------------------------------------------------

export interface Trademark {
  mark: string
  owner: string
  clients: Client[]
  /** What the mark labels, in our UI. */
  use: string
}

export const TRADEMARKS: Trademark[] = [
  {
    mark: `Apple logo`,
    owner: `Apple Inc.`,
    clients: [`desktop`, `marketing`, `web`],
    use: `label the "Sign in with Apple" button and the macOS download link`,
  },
  {
    mark: `App Store badge`,
    owner: `Apple Inc.`,
    clients: [`marketing`],
    use: `label the iOS App Store download link`,
  },
  {
    mark: `Google "G" logo`,
    owner: `Google LLC`,
    clients: [`android`, `desktop`, `web`],
    use: `label the "Sign in with Google" button`,
  },
  {
    mark: `Claude logo`,
    owner: `Anthropic PBC`,
    clients: [`android`, `desktop`, `ios`],
    use: `label the Claude coding agent in agent pickers and session views`,
  },
  {
    mark: `Codex logo`,
    owner: `OpenAI, L.L.C.`,
    clients: [`android`, `desktop`, `ios`],
    use: `label the Codex coding agent in agent pickers and session views`,
  },
  {
    mark: `Pi logo`,
    owner: `the Pi project`,
    clients: [`desktop`, `ios`],
    use: `label the Pi coding agent in agent pickers and session views`,
  },
  {
    mark: `Microsoft Windows logo`,
    owner: `Microsoft Corporation`,
    clients: [`marketing`],
    use: `label the Windows download link`,
  },
  {
    mark: `Tux, the Linux mascot`,
    owner: `Linus Torvalds (the LINUX trademark)`,
    clients: [`marketing`],
    use: `label the Linux download link`,
  },
]

export const TRADEMARK_STATEMENT = [
  `The marks below are reproduced NOMINATIVELY: each one identifies the product or platform it labels, so that a person can tell which sign-in provider, coding agent or download they are choosing. They are the property of their respective owners.`,
  `They are not open-source components and no open-source licence is claimed over them. Nothing in this file grants any right to use them. Exponential is not affiliated with, endorsed by, or sponsored by any of the owners named below.`,
]

// ---------------------------------------------------------------------------
// MPL-2.0 source availability (section 3.2)
// ---------------------------------------------------------------------------
//
// Reproducing the licence is not enough for MPL: a recipient of the binary must
// be told where to get the Source Code Form. The list of crates is DERIVED from
// inventory/rust.json (every component whose elected licences include MPL-2.0
// and which is actually linked), so it can never go stale — this map only
// supplies the URL, and the drift gate fails if a linked MPL crate has no entry.
//
// Build-only crates are excluded by the generator: `cbindgen` runs during the
// build and is never part of the distributed binary, so section 3.2 has no
// recipient to inform.

// A key ending in `*` matches by prefix — `lightningcss` publishes one npm
// package per platform triple, all built from the one repository, and twelve
// identical lines would be noise rather than information.
export const MPL_SOURCE_URLS: Record<string, string> = {
  // Rust (desktop)
  cssparser: `https://github.com/servo/rust-cssparser`,
  [`cssparser-macros`]: `https://github.com/servo/rust-cssparser`,
  [`dtoa-short`]: `https://github.com/upsuper/dtoa-short`,
  [`option-ext`]: `https://github.com/soc/option-ext`,
  dwrote: `https://github.com/servo/dwrote-rs`,
  // npm (web)
  [`lightningcss*`]: `https://github.com/parcel-bundler/lightningcss`,
  // npm (marketing)
  mediabunny: `https://github.com/Vanilagy/mediabunny`,
  [`@mediabunny/*`]: `https://github.com/Vanilagy/mediabunny`,
}

/** Exact key first, then the `*` prefix keys, longest prefix wins. */
export function mplSourceUrl(name: string): string | undefined {
  if (MPL_SOURCE_URLS[name]) return MPL_SOURCE_URLS[name]
  const prefixes = Object.keys(MPL_SOURCE_URLS)
    .filter((k) => k.endsWith(`*`))
    .sort((a, b) => b.length - a.length)
  for (const key of prefixes) {
    if (name.startsWith(key.slice(0, -1))) return MPL_SOURCE_URLS[key]
  }
  return undefined
}

export const MPL_STATEMENT = [
  `The components below are covered by the Mozilla Public License 2.0. Each is distributed UNMODIFIED, exactly as published by its author. Section 3.2 of that licence requires that recipients of a binary be informed of how to obtain the Source Code Form; the URL given for each component is where it may be obtained.`,
]

// ---------------------------------------------------------------------------
// Commercially licensed components
// ---------------------------------------------------------------------------
//
// `docs/third-party-licences.md` (section "The notices rule") is authoritative
// for what belongs here and for the dated determinations behind each entry.
// These must never appear in the open-source aggregate. The iOS and Android
// entries are not here — their collectors emit those components with an empty
// `licenses` array and the generator routes them into this section
// automatically.

export const COMMERCIAL: CuratedEntry[] = [
  {
    title: `Remotion`,
    clients: [`marketing`],
    body: [
      `The marketing site's hero animation is built with Remotion, which is source-available under the Remotion License rather than an open-source licence. Exponential qualifies under that licence's free tier, and the licence imposes no attribution requirement — this entry exists as our own audit trail. See docs/third-party-licences.md for the dated determination.`,
    ],
    reproduce: [
      { label: `Remotion License`, path: `docs/licences/remotion-LICENSE.txt` },
    ],
  },
]

// ---------------------------------------------------------------------------
// Components deliberately NOT in this build
// ---------------------------------------------------------------------------

export const NOT_INCLUDED: CuratedEntry[] = [
  {
    title: `GPL-licensed components patched out of this build`,
    clients: [`desktop`],
    body: [
      `The desktop application builds on gpui, which upstream Zed uses alongside two GPL-licensed crates: ztracing and zlog. Neither is part of this build. They are patched out and replaced by clean-room shims that reproduce only the API names the call sites need, and zlog does not appear in apps/desktop/Cargo.lock at all.`,
      `No GPL-licensed code is linked into any Exponential artifact. Where an upstream offered a choice that included a copyleft licence — self_cell offers "Apache-2.0 OR GPL-2.0-only" — the permissive branch was elected and is recorded in the licence election section above.`,
    ],
  },
]
