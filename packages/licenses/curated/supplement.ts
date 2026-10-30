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
  {
    title: `selfh.st/icons`,
    clients: [`android`, `desktop`, `ios`, `web`],
    body: [
      `EXP-924: the device icon picker offers three operating-system marks Lucide does not ship (os-apple, os-windows, os-linux). Their outlines come from selfh.st/icons (https://github.com/selfhst/icons, the apple-light, microsoft-windows-light and linux-light SVGs, kept unmodified under packages/icons/svg/). packages/icons scales each single path from its 512-unit viewBox into an 18-unit live area centred in Lucide's 24-unit grid and emits it into all four clients; that scaling is the only change made.`,
      `The collection is licensed CC-BY-4.0. The Apple logo, the Windows logo and Tux are trademarks of their respective owners, used nominatively to let a user label a machine by the operating system it runs; the licence below covers the icon files, not the marks.`,
      `The MCP catalog (Settings › MCP servers, web and desktop) draws the brand mark of each well-known hosted server from the same collection: the LIGHT SVGs of github, gitlab, linear, jira, notion, slack, sentry, stripe, paypal, supabase, cloudflare, netlify, figma, airtable, box, dropbox, posthog and todoist, vendored verbatim under packages/icons/brand/ (512-unit viewBox, white fills). packages/icons emits each one unchanged: as a React component on the web and as the file itself under apps/desktop/assets/icons/brand-<slug>.svg; sizing is the only thing a call site changes. Those marks are their owners' trademarks (listed under Trademarks), reproduced nominatively so a member can tell which service a server belongs to; the licence below covers the icon files, not the marks. iOS and Android have no MCP surface and bundle none of them.`,
    ],
    reproduce: [
      { label: `CC-BY-4.0 — selfh.st/icons`, path: `packages/licenses/texts/spdx/CC-BY-4.0.txt` },
    ],
  },
]

// ---------------------------------------------------------------------------
// Bundled data — MIT
// ---------------------------------------------------------------------------
//
// EXP-551: the emoji dataset is not a package on any client. packages/emoji
// reads emojibase-data (a devDependency of that generator only) and emits one
// JSON of names, shortcodes, tags and skin-tone variants into all four apps,
// so no collector will ever see it — this entry is the attribution route.

export const DATA: CuratedEntry[] = [
  {
    title: `Emojibase`,
    clients: [`android`, `desktop`, `ios`, `web`],
    body: [
      `Every client bundles an emoji dataset (labels, GitHub-style shortcodes, search tags and skin-tone variants) for its emoji picker and :shortcode autocomplete. It is not consumed as a package: packages/emoji projects the emojibase-data package into one JSON file (apps/web/src/lib/emoji.generated.json, apps/ios/Exponential/Resources/emoji.json, apps/android/app/src/main/assets/emoji.json, apps/desktop/assets/emoji.json), so the same data is reproduced in all four builds. The emoji themselves are Unicode characters; only the descriptive data comes from Emojibase.`,
    ],
    reproduce: [
      { label: `MIT License — Emojibase`, path: `packages/emoji/LICENSE-emojibase.txt` },
    ],
  },
]

// ---------------------------------------------------------------------------
// Vendored source
// ---------------------------------------------------------------------------
//
// None of these is a package in any dependency graph. gpui-markdown-editor
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
  // NEW-74: exponentialTalk's voice. Not in the binary: the speech worker
  // downloads it on first use from the publishers, verifies every file
  // against a pinned sha256 and loads it at runtime; the macOS bundle ships
  // the CrispASR runtime itself (apps/desktop/scripts/build-macos-app.sh;
  // docs/third-party-licences.md, NEW-74).
  {
    title: `exponentialTalk's voice runtime (downloaded at first use; bundled on macOS)`,
    clients: [`desktop`],
    body: [
      `The macOS application bundle contains the CrispASR v0.8.45 Metal runtime (libcrispasr and ggml dylibs, unmodified, re-signed). On Linux and Windows, when a member turns on Speak replies, the desktop application's speech worker downloads, from the publishers' own servers, the CrispASR v0.8.45 runtime for the platform (https://github.com/CrispStrobe/CrispASR, MIT; the prebuilt libcrispasr with ggml, MIT, and the bundled components its third-party notices list; built without espeak-ng) ; on every platform it downloads the Qwen3-TTS 12 Hz 0.6B Base model with its 12 Hz tokenizer (https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-Base, Apache-2.0; GGUF conversions by cstr for CrispASR, Apache-2.0). On Linux it also fetches NVIDIA's CUDA runtime (nvidia-cuda-runtime-cu12 12.9.79) and cuBLAS (nvidia-cublas-cu12 12.9.2.10) from NVIDIA's own wheels on PyPI; they are governed by the NVIDIA CUDA Toolkit End User License Agreement (https://docs.nvidia.com/cuda/eula/), whose text is kept beside them, and are not redistributed by Exponential. The CrispASR licence and notices are reproduced below; the Apache License 2.0 is reproduced above.`,
      `NEW-78: when a member turns on Push to talk, the worker downloads, on first use, the transcriber: Parakeet TDT 0.6B v3 by NVIDIA (https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3, licensed under CC BY 4.0, https://creativecommons.org/licenses/by/4.0/; converted to GGUF and quantised to Q8_0 by cstr for CrispASR, https://huggingface.co/cstr/parakeet-tdt-0.6b-v3-GGUF; not modified by Exponential) and the Silero VAD v6.2 (https://github.com/snakers4/silero-vad, MIT, Silero Team; ggml conversion by ggml-org, https://huggingface.co/ggml-org/whisper-vad, MIT). On a Linux or Windows machine without a GPU driver it downloads CrispASR v0.8.45's CPU-only runtime instead of the GPU one; on Linux that archive carries OpenBLAS (BSD-3-Clause) with libgfortran and libquadmath (GCC Runtime Library Exception / LGPL-2.1), listed in the CrispASR notices below and not redistributed by Exponential. On Linux both CrispASR archives (CUDA and CPU-only) carry libgomp.so.1, the GNU OpenMP runtime (https://gcc.gnu.org/, GPL-3.0-or-later WITH GCC-exception-3.1, the GCC Runtime Library Exception: https://www.gnu.org/licenses/gpl-3.0.html, https://www.gnu.org/licenses/gcc-exception-3.1.html), unmodified from Ubuntu 22.04's gcc-12, whose corresponding source is available from Ubuntu's source archive (apt-get source gcc-12), from https://gcc.gnu.org/ and from the CrispASR maintainers as their notices below state; it is fetched by the member's machine, not redistributed by Exponential.`,
    ],
    reproduce: [
      { label: `MIT License — CrispASR (incl. ggml)`, path: `packages/licenses/texts/crispasr/LICENSE.txt` },
      { label: `CrispASR third-party notices`, path: `packages/licenses/texts/crispasr/THIRD_PARTY_NOTICES.txt` },
    ],
  },
  // VAPP-87: the web app bundles the Exponential UI catalog package
  // (`@exponential_app/ui`, through `@exponential_app/ui-react` on the kitchen
  // sink route): the vendored A2UI v0.9 schemas and the json-render wording
  // now reach a client build, so the two Apache-2.0 sources get their rows
  // (docs/third-party-licences.md, VAPP-85 section). VAPP-90: the desktop
  // app links the Rust core (`exponential-ui`, which EMBEDS the generated
  // catalog — json-render-derived descriptions included — and the resolved
  // themes in the binary), so `desktop` joins. VAPP-88: the iOS painter
  // (`packages/exponential-ui-swift`) ships the same Rust core inside its
  // xcframework, so `ios` joins. VAPP-89: the Android Compose painter
  // (`packages/exponential-ui-compose`) loads the same Rust core as the
  // `exponential-ui-ffi` `.so` inside its AAR, so `android` joins.
  {
    title: `A2UI specification schemas (vendored, Exponential UI)`,
    clients: [`web`, `desktop`, `ios`, `android`],
    body: [
      `The Exponential UI catalog package (packages/exponential-ui, bundled by the web app through the React renderer) carries the A2UI v0.9 specification's JSON schemas, copied byte for byte from https://github.com/google/A2UI (tag v0.9, commit 19919ef4c8ad3185867f70386fa4669284d7714c) into packages/exponential-ui/vendor/a2ui/v0_9/. They are not modified: the basic catalog is mapped onto Exponential UI's core catalog, so Apache-2.0 section 4(b) does not apply. The repository's LICENSE is reproduced below.`,
    ],
    reproduce: [
      {
        label: `Apache License 2.0 — A2UI (Google LLC)`,
        path: `packages/exponential-ui/vendor/a2ui/LICENSE.txt`,
      },
    ],
  },
  {
    title: `json-render component descriptions (derived wording, Exponential UI)`,
    clients: [`web`, `desktop`, `ios`, `android`],
    body: [
      `The model-facing descriptions and the prop vocabulary of the 36 shadcn-named components in packages/exponential-ui/catalog/core.catalog.json were derived from the @json-render/shadcn catalog of https://github.com/vercel-labs/json-render (0.21.0, commit fc2a696a50a30cb30c878ab1eb65e102487eea0f, Apache-2.0, Vercel, Inc.): rewritten into Exponential UI's schema format and re-edited sentence by sentence, no code copied. The attribution is kept because the wording is a derivative; the licence is reproduced below.`,
    ],
    reproduce: [
      {
        label: `Apache License 2.0 — json-render (Vercel, Inc.)`,
        path: `packages/exponential-ui/vendor/json-render/LICENSE.txt`,
      },
    ],
  },
  // VAPP-162: the QrCode native's matrix is a port of Nayuki's QR Code
  // generator (MIT) in the catalog package (TS) and the Rust core, so it
  // reaches every client the two rows above reach.
  {
    title: `QR Code generator library (ported, Exponential UI)`,
    clients: [`web`, `desktop`, `ios`, `android`],
    body: [
      `The QrCode native's matrix (packages/exponential-ui/src/qr-code.ts and the Rust core's port) is a port of Project Nayuki's QR Code generator library (https://www.nayuki.io/page/qr-code-generator-library, MIT), narrowed to byte mode at an exact error-correction level. The MIT notice is reproduced below.`,
    ],
    reproduce: [
      {
        label: `MIT License — QR Code generator library (Project Nayuki)`,
        path: `packages/exponential-ui/vendor/nayuki-qr/LICENSE.txt`,
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
  // The MCP catalog's brand marks (packages/icons `brand`, contract
  // `mcpCatalog`): web + desktop only, the natives have no MCP surface.
  {
    mark: `GitHub logo (the Invertocat)`,
    owner: `GitHub, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `GitLab logo (the Tanuki)`,
    owner: `GitLab Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Linear logo`,
    owner: `Linear Orbit, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Jira logo`,
    owner: `Atlassian Pty Ltd`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Notion logo`,
    owner: `Notion Labs, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Slack logo`,
    owner: `Slack Technologies, LLC`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Sentry logo`,
    owner: `Functional Software, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Stripe logo`,
    owner: `Stripe, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `PayPal logo`,
    owner: `PayPal, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Supabase logo`,
    owner: `Supabase, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Cloudflare logo`,
    owner: `Cloudflare, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Netlify logo`,
    owner: `Netlify, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Figma logo`,
    owner: `Figma, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Airtable logo`,
    owner: `Formagrid, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Box logo`,
    owner: `Box, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Dropbox logo`,
    owner: `Dropbox, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `PostHog logo`,
    owner: `PostHog, Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
  {
    mark: `Todoist logo`,
    owner: `Doist Inc.`,
    clients: [`desktop`, `web`],
    use: `label its hosted MCP server in the MCP catalog and on a team's server rows (selfh.st/icons light SVG, unmodified)`,
  },
]

export const TRADEMARK_STATEMENT = [
  `The marks below are reproduced NOMINATIVELY: each one identifies the product or platform it labels, so that a person can tell which sign-in provider, coding agent, download or hosted MCP server they are choosing. They are the property of their respective owners.`,
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
  // The Exponential UI FFI facade's binding layer (uniffi, uniffi_* crates).
  [`uniffi*`]: `https://github.com/mozilla/uniffi-rs`,
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

export const COMMERCIAL: CuratedEntry[] = []

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
