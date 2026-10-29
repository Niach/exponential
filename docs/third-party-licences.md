# Third-party licences — the non-OSS components

The repository itself is Apache-2.0 (`LICENSE`, EXP-352) and the bulk of its
dependency graph is MIT/Apache/BSD/ISC, which the notice generator (EXP-375)
attributes mechanically. This file covers the handful of components a generator
**cannot** decide for us: source-available and closed-source code whose terms
have to be read, applied to our own entity, and re-checked over time.

Every determination below is dated. Re-open this file when headcount changes,
when a pinned version moves across a major boundary, or when a new distribution
surface appears.

## Published images install only their own workspace (EXP-380, REV-2)

Every image the repo builds runs its published `bun install` with
`--filter '<its workspace>'` (`@exp/web`, `@exp/steer-relay`,
`@exp/push-relay`). Unfiltered, the install pulls in every other workspace's
production dependencies — marketing, the styleguide, the shots pipeline — and
hands them to whoever pulls the image, including the self-hosters of the public
`ghcr.io/niach/exponential-web` and `ghcr.io/niach/exponential-steer-relay`
images. None of those apps' code runs in an image, and the per-client notices
below only inventory what each client actually ships, so an unscoped install
would redistribute components no notice covers.

The web image's builder stage deliberately still installs everything: it is
discarded, only `apps/web/.output` is copied out of it, and `@exp/widget`'s
build reaches across the workspace. The relays have no build step, so their
builder's install IS the published one (the runtime stage copies its
`node_modules` wholesale) and filtering it is the whole fix.

`apps/web/src/lib/third-party-licences.test.ts` gates **every** image the repo
builds and asserts the `node_modules` provenance each gate assumes — so an image
that starts copying a builder's `node_modules` fails the test until someone
re-reads which install is published.

## libvips inside sharp: copyleft in the public web image (EXP-854)

### What we depend on

`apps/web/package.json` depends on `sharp` (0.34.5) for EXP-854: the MCP tool
`exponential_attachments_get` bounds the image it inlines into an agent's
context (`apps/web/src/lib/mcp/inline-image.ts`). `sharp` itself is Apache-2.0,
and so is every `@img/sharp-<platform>` binding prebuild. What needs a reading
is the layer under them: the ten `@img/sharp-libvips-<platform>` packages,
each a prebuilt libvips shared library declared **LGPL-3.0-or-later**. They are
installed by platform, so the published `ghcr.io/niach/exponential-web` image
carries the Linux ones, and that image is also the self-host distribution.

`packages/shots` already depended on `sharp`, but that is a local capture tool
which no image ships, so this is the first time the prebuilds are redistributed.

### Determination 1: shipping the prebuilt libvips (2026-09-12)

**Permitted, and the obligations are discharged by the notice.**

LGPL-3.0 section 4 allows conveying a Combined Work under terms of our choosing
provided the recipient can relink it against a modified libvips. We use libvips
through `sharp`'s native binding, which loads it as a **separate, unmodified
shared library**, which is exactly the case section 4(d)(0) is written for: the
library is its own file in `node_modules`, replacing it needs no build of ours,
and we ship it byte-identical to the npm prebuild, at a pinned version anyone
can rebuild from libvips upstream. What is owed on top of that is notice
(sections 4(a) and 4(b)): the licence text and the statement that libvips is
used and covered by the LGPL. `apps/web/public/NOTICES.txt` reproduces the full
LGPL-3.0 text and names all ten components, and the image serves it at
`/NOTICES.txt` next to `/app/LICENSE` and `/app/NOTICE`.

The LGPL is an open source licence granted to everyone downstream, so passing
the image on carries the rights with it. Nothing is sublicensed and nothing is misstated.

**Re-read this the moment we stop using the prebuilds as shipped**, that is, if
we ever vendor, patch or statically link libvips. Static linking moves us out of
section 4(d)(0) and into 4(d)(1), which owes the recipient the object code to
relink with.

### Determination 2: where it belongs in the notices (2026-09-12)

**The aggregate, not the commercially licensed section.**

The rule further down routes source-available and closed-source components into
a dedicated section because their terms were never granted to our recipients.
The LGPL is neither: it is an OSI-approved open source licence whose terms flow
to everyone who receives the image. It gets its own licence heading inside the
ordinary generated body, with its text reproduced in full, which is what the
generator already does for every id it meets. Do not move it.

### How the inventory stays honest

`bun run --filter @exp/licenses collect:npm` refused this dependency twice
before it was recorded, which is the gate working:

1. No SPDX template for `LGPL-3.0-or-later`. The id is now in
   `scripts/fetch-texts.ts` and `texts/spdx/LGPL-3.0-or-later.txt` holds the
   canonical text from the pinned SPDX tag. Nothing here is hand-written.
2. The musl families (`@img/sharp-libvips-linuxmusl-*`,
   `@img/sharp-linuxmusl-*`) install on no host we collect from, so they have
   no on-disk evidence and would drift between a macOS and a Linux run. Both
   carry a reviewed `PLATFORM_LICENCE_FALLBACK` entry in
   `scripts/collect-npm.ts`, verified against the registry metadata
   (LGPL-3.0-or-later for the libvips prebuilds, Apache-2.0 for the bindings).

## Closed-source Google binaries — mobile only

Found in the same EXP-262 audit. All three are shipped inside the store builds,
none is open source, and none may appear in the MIT/Apache aggregate.

| Component | Client | Pulled in by | Terms |
| --- | --- | --- | --- |
| `GoogleAppMeasurement` | iOS | `firebase-ios-sdk` 11.15.0 (`apps/ios/Tuist/Package.resolved`) | Closed-source binary framework under Google's own terms — `github.com/google/GoogleAppMeasurement` ships no OSS licence for the binaries |
| `google-ads-on-device-conversion-ios-sdk` | iOS | `firebase-ios-sdk` 11.15.0 | Closed-source binary, Google Ads terms |
| `com.google.android.play:app-update-ktx` 2.1.0 | Android | `apps/android/gradle/libs.versions.toml` | Play Core Software Development Kit Terms of Service |

These are store-build-only and never reach the web app or the public image, so
there is no redistribution question — only a notices question, answered below.

### Android: the resolved graph carries ten, not one — 2026-07-31

Building EXP-375's collector against the real `productionRelease` runtime
configuration (rather than against `libs.versions.toml`, which names none of
them) surfaced that `app-update-ktx` is not alone. Ten resolved artifacts
declare proprietary terms in their POMs; folding them into the Apache aggregate
would assert terms Google never granted.

| Artifact | Terms |
| --- | --- |
| `com.google.android.play:app-update-ktx` 2.1.0 | Play Core SDK Terms of Service |
| `com.google.android.play:app-update` 2.1.0 | Play Core SDK Terms of Service |
| `com.google.android.play:core-common` 2.0.3 | Play Core SDK Terms of Service |
| `com.google.android.gms:play-services-base` 18.0.1 | Android SDK License |
| `com.google.android.gms:play-services-basement` 18.3.0 | Android SDK License |
| `com.google.android.gms:play-services-cloud-messaging` 17.2.0 | Android SDK License |
| `com.google.android.gms:play-services-stats` 17.0.2 | Android SDK License |
| `com.google.android.gms:play-services-tasks` 18.1.0 | Android SDK License |
| `com.google.firebase:firebase-iid-interop` 17.1.0 | Android SDK License |
| `com.google.firebase:firebase-measurement-connector` 19.0.0 | Android SDK License |

The first three come in through `play-app-update-ktx`; the rest through
`firebase-messaging` via `compose-bom`/`firebase-bom` expansion. The collector
emits all ten with an empty `licenses` array, which is the marker that routes a
component into the commercially-licensed section — see the notices rule below.
This list is not maintained by hand: it is whatever the resolved graph declares,
and `apps/web/src/lib/licenses.test.ts` fails if one of them ever appears in the
open-source aggregate.

## The notices rule

Whatever the generator (EXP-375) emits, these components go in a dedicated
**"Commercially licensed components"** section of the per-client notices, and
**never** in the MIT/Apache aggregate — folding a source-available or
closed-source component into a list headed by a permissive licence asserts terms
that were never granted.

- **iOS** (`apps/ios/Exponential/Resources/NOTICES.txt`) — `GoogleAppMeasurement`
  and `google-ads-on-device-conversion-ios-sdk`, identified as closed-source
  Google binaries with a pointer to Google's terms (there is no licence body to
  reproduce).
- **Android** (`apps/android/app/src/main/assets/NOTICES.txt`) — the ten
  proprietary Google artifacts tabulated above, under the Play Core SDK Terms of
  Service and the Android SDK License.
- **Web** (`apps/web/public/NOTICES.txt`), **marketing**
  (`apps/marketing/public/NOTICES.txt`) and **desktop** — nothing from this
  file, and the section should be absent rather than empty.

This sits alongside, and follows the same rule as, EXP-375's trademarks section:
marks and non-OSS components each get their own heading, never an OSS one.

### Brand marks in the MCP catalog (2026-09-29)

Settings › MCP servers (web + desktop) labels each well-known hosted server
with its REAL brand mark. The files are the LIGHT SVGs of selfh.st/icons
(CC-BY-4.0, attribution carried in the selfh.st/icons entry of the notices),
vendored verbatim under `packages/icons/brand/` and emitted unchanged by
`packages/icons` (a React component per slug for the web, the file itself as
`apps/desktop/assets/icons/brand-<slug>.svg` for the desktop). The licence
covers the icon FILES; the marks themselves are their owners' trademarks and
are listed one by one under the notices' Trademarks heading with the owner
and the nominative use. Rules that follow from that: never recolour, restyle
or redraw a mark (sizing is the only change a call site may make), never fold
one into the Lucide grid, never ship one where no such server is offered (the
natives have no MCP surface and bundle none). Adding a catalog entry = a light
SVG under `brand/`, a row in `icons.json` `brand` (with `owner`), a row in
`contract.json` `mcpCatalog`, a `TRADEMARKS` row in
`packages/licenses/curated/supplement.ts`, then the three generators.

### The cua driver, gone (EXP-1273, 2026-10-19)

`apps/desktop/crates/computer` linked the cua driver (trycua/cua, MIT) from
EXP-1196 to EXP-1273. It links nothing of it any more: computer use is a
thin foreground backend of our own on crates already in the tree (`x11rb`,
`core-graphics` and the `objc2` family, `windows-sys`, `image`), so the git
dependency, its `uniffi` binding layer, the fetched `cua-LICENSE.txt`
reproduction and the curated supplement rows for them went with it; nothing
of cua was ever copied into this repository.

### Codex, a managed download (EXP-1232, 2026-10-07)

The desktop app and the `exponential` CLI no longer look for a `codex` the
person installed: `apps/desktop/crates/coding/src/managed_codex.rs` pins ONE
upstream release of OpenAI's Codex CLI (openai/codex, Apache-2.0) by tag and
by the sha256 of each target's tarball, and the person's machine downloads
that tarball from OpenAI's GitHub release the first time Codex is needed.
Nothing of Codex is copied into this repository, compiled into any binary,
or carried in any release asset or image of ours — the bytes travel from
OpenAI to the user's data dir, so there is no redistribution and nothing to
add to the notices. The pinned digests are an integrity check, not a
licence determination. Bumping the pin = the version, the tag and the six
digests in that one file.

### Exponential UI: the vendored A2UI basic catalog and the json-render wording (VAPP-85, 2026-10-07)

`packages/exponential-ui` (the `@exponential_app/ui` catalog package, private,
unpublished until VAPP-91) carries two Apache-2.0 sources:

- **A2UI** (google/A2UI, Apache-2.0, Google LLC): the v0.9 specification's
  JSON schemas, copied byte-for-byte into
  `packages/exponential-ui/vendor/a2ui/v0_9/` from git tag `v0.9`, commit
  `19919ef4c8ad3185867f70386fa4669284d7714c`, with the repository `LICENSE`
  beside them and every file's sha256 pinned by `vendor.test.ts`. They are
  never edited: the basic catalog is MAPPED onto ours (`catalog/basic-map.json`),
  not modified, so Apache-2.0 section 4(b) (a statement of changes) does not
  apply; 4(a)/4(c) are met by the vendored licence and the untouched headers.
- **json-render** (vercel-labs/json-render, Apache-2.0, Vercel, Inc.): the
  model-facing descriptions and the prop vocabulary of the 36 shadcn-named
  components in `catalog/core.catalog.json` were DERIVED from
  `packages/shadcn/src/catalog.ts` (`@json-render/shadcn` 0.21.0, commit
  `fc2a696a50a30cb30c878ab1eb65e102487eea0f`): rewritten into our schema
  format and re-edited sentence by sentence, no code copied. The attribution
  is kept anyway (`packages/exponential-ui/vendor/json-render/`, licence
  verbatim) because the wording is a derivative.

- **shadcn/ui** (shadcn-ui/ui, MIT, shadcn): the built-in `neutral` theme
  (`packages/exponential-ui/themes/neutral.theme.json`, VAPP-92) carries the
  stock "neutral" palette VALUES of shadcn's `globals.css` (OKLCH converted to
  hex) and recipes that describe the stock component look in our own recipe
  format. No code is copied; colour values and a visual description are not a
  copyrightable work, so no notice is owed. Recorded here for provenance.

**Which clients bundle it (VAPP-87, 2026-10-07):** the web app, through
`@exponential_app/ui-react` (the React renderer, `packages/exponential-ui-react`)
on its `/exponential-ui-kitchen-sink` route, which imports the catalog JSON
and the built-in themes. The two Apache-2.0 sources therefore have one
`VENDORED` row each in `packages/licenses/curated/supplement.ts` with
`clients: ["web"]`, reproducing the vendored `LICENSE.txt` files. The React
renderer itself adds no third-party source: its natives are Radix behaviour
(MIT, in the npm inventory) painted from the theme's recipes, and the shadcn
primitive set that moved from `@exp/ui` into `src/primitives/` was already
ours (MIT-derived shadcn code, recorded with the web app's dependencies).
The Solid renderer (FEED-128, `@exponential_app/ui-solid`,
`packages/exponential-ui-solid`) adds none either: its runtime set is
`solid-js` 1.9 (MIT), `@floating-ui/dom` 1.7 (MIT), `aria-hidden` 1.2 (MIT)
and `lucide-solid` 0.544 (ISC), each declared in its npm tarball (verified
2026-10-21; FEED-131 dropped `@kobalte/core`); no client bundles it (only its
npm package, the ui.exponential.at guide and the `web-solid` sample), so no
inventory changes. Its primitives (FEED-131) are ports of our own React set,
and its `styles.css` is the React package's compiled sheet. The catalog
package's new runtime dependencies (FEED-131, the shared primitive recipes),
`class-variance-authority` 0.7 (Apache-2.0), `clsx` 2.1 (MIT) and
`tailwind-merge` 3 (MIT), were already in the web app's inventory through
`@exponential_app/ui-react`, so no inventory changes either.
The desktop app (VAPP-90), the iOS SwiftUI painter (VAPP-88,
`packages/exponential-ui-swift`, whose xcframework embeds the same Rust core
with the generated catalog and themes) and the Android Compose painter
(VAPP-89, `packages/exponential-ui-compose`, whose AAR ships the same core as
the `exponential-ui-ffi` `.so`) now carry them too, so both rows read
`clients: ["web", "desktop", "ios", "android"]`. The Compose painter AAR's
runtime third-party set is Jetpack Compose (Apache-2.0, already in the Android
app's inventory) plus JNA 5.17.0 (`net.java.dev.jna:jna@aar`, dual
LGPL-2.1-or-later / Apache-2.0; we elect Apache-2.0) and, for Video /
AudioPlayer playback (VAPP-103), AndroidX Media3 1.11.1
(`media3-exoplayer` + `media3-ui`, Apache-2.0, the same line the Android
app already ships and inventories); Robolectric and
Roborazzi are test-only. Since EXP-1280 (the plan card's inline surface) the
Exponential Android app links the painter (`:ui-compose`) itself, so JNA and
the `.so` are in its `NOTICES.txt` inventory (`collect:android` picks JNA up
from the app's licence report).
**When a native painter (VAPP-88/89/90) bundles the catalog JSON or the
built-in themes into iOS, Android or the desktop app, add that client to the
two rows' `clients` and regenerate the notices** — the same rule as the
desktop's vendored crates above.

### Exponential UI: the conformance fonts (round 2, 2026-10-08)

`packages/exponential-ui/conformance/fonts/` holds the fonts the conformance
harness lays text out with on every renderer, so web and native measure the
same glyphs:

- **Nunito** 3.602 (googlefonts/nunito, SIL OFL 1.1, The Nunito Project
  Authors; no Reserved Font Name): seven static instances made from Google
  Fonts' `Nunito[wght].ttf` and `Nunito-Italic[wght].ttf` with
  `fonttools varLib.instancer --update-name-table`. OFL allows modified
  versions under the same licence; with no Reserved Font Name the family name
  may stay.
- **Fira Code** 6.2 (tonsky/FiraCode, SIL OFL 1.1, The Fira Code Project
  Authors): the release's TTFs, unmodified.

Each family's licence sits beside the files (`OFL-Nunito.txt`,
`OFL-FiraCode.txt`), which is what OFL section 2 asks of any copy. Only the
harness and the renderers' conformance tests load them. No app bundle, native
build or notice uses them, and `release/prepare-npm.ts` leaves the folder out
of the npm package. The public web image does carry them, because its runtime
stage copies `packages/` whole; the licence files travel with them, so that
copy is covered too.

## How this is enforced — EXP-375

The rule above is mechanical, not aspirational. `packages/licenses` generates
all five `NOTICES.txt` from the real dependency graphs, and a component is
routed into the commercially-licensed section by having an EMPTY `licenses`
array in its inventory entry — see `packages/licenses/README.md`.

Determinations that a collector cannot derive live in
`packages/licenses/curated/overrides.ts`, each one dated and carrying its
evidence, next to this file. An override that stops matching anything is a hard
error, so a determination cannot outlive the dependency it was written for.

## Shipping the licence files — EXP-376

Apache-2.0 section 4(a) requires giving recipients a copy of the License, and
4(d) requires propagating our `NOTICE`. Every distributed artifact now carries
both:

| Artifact | Where |
| --- | --- |
| macOS `.app` | `Contents/Resources/{LICENSE,NOTICE,NOTICES.txt}` |
| Linux `.AppImage` | `usr/share/doc/exponential/{LICENSE,NOTICE,NOTICES.txt}` |
| GitHub Release (incl. the bare Windows `.exe`) | `LICENSE`, `NOTICE`, `NOTICES.txt` alongside `SHA256SUMS.txt` |
| `ghcr.io/niach/exponential-web` | `/app/LICENSE`, `/app/NOTICE`; the inventory is served at `/NOTICES.txt` |
| `ghcr.io/niach/exponential-steer-relay` | `/app/LICENSE`, `/app/NOTICE` (REV-2) |
| Desktop binary | `include_str!` — `apps/desktop/crates/app/src/licenses.rs` |

The steer relay image ships no per-dependency inventory: it is not a client, it
serves no HTTP surface to host one, and `packages/licenses` has no collector for
it. Its whole runtime graph is `hono` + `zod` + `@exp/steer-ticket`, all MIT or
ours, so what is outstanding is reproducing two MIT copyright notices — a real
obligation, but a small one, and
deliberately left to whoever adds a sixth collector.

The desktop binary embeds `NOTICES.txt` because `assets.rs` compiles in ~3.6 MB
of OFL-licensed Font Software and Lucide's ISC geometry while the `LICENSE.txt`
files sitting beside them are outside the rust-embed include list. The generated
notice reproduces both bodies in full, which discharges OFL section 2 and the
ISC notice without widening that list.

## exponentialTalk: the voice and its runtime (NEW-53)

Retired by NEW-74. NEW-53 linked sherpa-onnx 2.0.0 (Apache-2.0, with ONNX
Runtime and its native dependencies) statically through a vendored
`sherpa-onnx-sys`, and downloaded the Kokoro v1.0 voice at runtime. Both are
gone: nothing of sherpa-onnx is linked any more, the vendored crate and its
licence texts are deleted, and the worker removes the old Kokoro download.

## exponentialTalk: one voice engine on the GPU (NEW-74)

### What we download, and the one piece we ship (macOS)

On Linux and Windows the speech worker fetches all of these on first use
from the publishers' own servers; on macOS the CrispASR runtime instead ships
INSIDE the signed, notarized app bundle (`Contents/Frameworks`, put there by
`apps/desktop/scripts/build-macos-app.sh` from the same pinned release asset,
re-signed with our identity) and only the models are fetched. The worker checks each download against a pinned sha256 BEFORE it unpacks or
loads it (`apps/desktop/crates/etalk-crispasr`: `platform.rs`, `models.rs`), keeps
only the named files, and re-hashes every library before `dlopen`. A
mismatch deletes the file and the voice backs off (1 min doubling to 1 h).

| what | version | licence | where from | platforms |
|---|---|---|---|---|
| CrispASR runtime: `libcrispasr` + ggml (`libggml*`), plus libgomp (GCC Runtime Library Exception) and the reference BLAS on Linux | v0.8.45 release assets | MIT (its `THIRD_PARTY_NOTICES.txt` lists the rest; built without espeak-ng, the only GPL component it knows) | github.com/CrispStrobe/CrispASR releases | Linux x86_64 (CUDA) and Windows x86_64 (Vulkan): downloaded; macOS arm64 (Metal): shipped in the bundle |
| NVIDIA CUDA runtime (`libcudart.so.12`) and cuBLAS (`libcublas.so.12`, `libcublasLt.so.12`) | 12.9.79 / 12.9.2.10 | NVIDIA CUDA Toolkit EULA (proprietary, redistributable); its text is kept beside the files | NVIDIA's own wheels on PyPI (`nvidia-cuda-runtime-cu12`, `nvidia-cublas-cu12`) | Linux x86_64 only |
| Qwen3-TTS 12 Hz 0.6B Base, F16 GGUF | cstr/qwen3-tts-0.6b-base-GGUF @ 94072f5 | Apache-2.0 | Hugging Face | all three |
| Qwen3-TTS 12 Hz tokenizer (codec), F16 GGUF | cstr/qwen3-tts-tokenizer-12hz-GGUF @ 344e279 | Apache-2.0 | Hugging Face | all three |
| NEW-78: CrispASR CPU-only runtime (`libcrispasr` + ggml; on Linux with libgomp (GCC Runtime Library Exception), OpenBLAS (BSD-3-Clause), libgfortran (GCC Runtime Library Exception) and libquadmath (LGPL-2.1)) | v0.8.45 release assets | MIT (+ the listed) | github.com/CrispStrobe/CrispASR releases | Linux x86_64 and Windows x86_64 without a GPU driver, for push to talk only |
| NEW-78: GNU OpenMP runtime `libgomp.so.1` (inside both Linux CrispASR archives, the CUDA and the CPU-only one; unmodified, from Ubuntu 22.04's `gcc-12`) | as built into CrispASR v0.8.45 | GPL-3.0-or-later WITH GCC-exception-3.1 (GCC Runtime Library Exception); corresponding source: `apt-get source gcc-12`, gcc.gnu.org, or the CrispASR maintainers (their `THIRD_PARTY_NOTICES.txt`) | github.com/CrispStrobe/CrispASR releases | Linux x86_64 only |
| NEW-78: Parakeet TDT 0.6B v3 (NVIDIA), Q8_0 GGUF | cstr/parakeet-tdt-0.6b-v3-GGUF @ dff69b0 | **CC BY 4.0** (attribution + "converted and quantised by cstr" in the notices) | Hugging Face | all three |
| NEW-78: Silero VAD v6.2, ggml | ggml-org/whisper-vad @ 9ffd54a | MIT | Hugging Face | all three |

Sizes of the first-use download: Linux ~2.97 GB (runtime 196 MB, cuBLAS
581 MB, model 1.83 GB, codec 358 MB), macOS ~2.19 GB (the models only; the
bundle grows by ~25 MB), Windows ~2.28 GB.

On disk: Linux ~3.29 GB unpacked (runtime + cuBLAS 1.09 GB, models
2.19 GB, built-in clips 1 MB), macOS ~2.19 GB plus the ~25 MB in the
bundle, Windows ~2.3 GB.

### On disk: `{data_dir}/talk/`

| path | what | written by |
|---|---|---|
| `crispasr-0.8.45/` | the runtime libraries, their licences, one `.<artifact id>.sha256` manifest per artifact (Linux, Windows; macOS reads `Contents/Frameworks` of the bundle instead) | `crispasr::artifact::ensure` |
| `crispasr-0.8.45-cpu/` | NEW-78: the CPU-only runtime, where no GPU driver exists (Linux, Windows) | same |
| `models/` | `qwen3-tts-12hz-0.6b-base.gguf`, `qwen3-tts-tokenizer-12hz.gguf`, NEW-78's `parakeet-tdt-0.6b-v3-q8_0.gguf` (674 MB) and `ggml-silero-v6.2.0.bin` (0.9 MB), and their manifests | same |
| `voices/` | `builtin-<language>.wav`, the built-in clips written out for CrispASR (rewritten when they differ) | `voice::builtin::materialize` |
| `no-gpu.txt` | the "this GPU and driver were too slow" verdict (fingerprint + time, 30 days) | the speech worker |
| `<artifact id>.part` | a download in progress (deleted on failure or after unpacking) | `crispasr::artifact::download` |
| `<file>.tmp` | a file being unpacked (renamed into place when complete) | `crispasr::artifact::write_kept` |

The NEW-53 `kokoro-multi-lang-v1_0/` directory is deleted by the worker on
start. #NEW-63: the IDE deletes the whole `{data_dir}/talk/` once an
account's settings move its voice device away from this machine and no
account signed in here still names it (`{data_dir}/voice-device-claims.json`),
and sweeps leftover `*.part` / `*.tmp` files whenever the speech worker shuts
down; both wait on the worker's lock (`talk::SpeechClient::remove_voice_data`,
`talk::SpeechClient::remove_partial_downloads`).

The built-in reference clips (`apps/desktop/crates/etalk-engine/assets/`, in the
binary, ~0.9 MB) are our own output of Qwen3-TTS 0.6B CustomVoice
(Apache-2.0), preset speaker `ryan`; no person's voice is in them (see the
README there). A member's own reference clip never leaves the voice device.

### Determination (2026-10-26)

Permitted, with attribution. MIT and Apache-2.0 allow use and
redistribution; on Linux and Windows we redistribute none of it (the user's
machine fetches it), on macOS we redistribute the MIT CrispASR runtime inside
the bundle, and the desktop notices reproduce CrispASR's licence and
third-party notices either way ("exponentialTalk's voice runtime"). The NVIDIA libraries
are fetched by the user's machine from NVIDIA's distribution under NVIDIA's
EULA, not shipped by Exponential. No GPL: CrispASR's espeak-ng path is off in
these builds and its phonemizer is not used by Qwen3-TTS. On macOS library
validation stays ON (review of #1237, 2026-10-26): no
`disable-library-validation` entitlement; the bundled dylibs carry our
signature and are loaded by absolute path from the running bundle only.
CrispASR watermarks every synthesized clip (an inaudible AI-content mark, EU
AI Act Art. 50(2)); it stays on. NEW-78 (2026-10-26): Parakeet is CC BY 4.0,
which permits use with attribution and a note of changes: the notices name
NVIDIA, the licence, cstr's GGUF conversion and Q8_0 quantisation; we change
nothing and redistribute nothing (the voice device fetches it). Silero and its
ggml conversion are MIT. The CPU-only runtime's GCC runtime libraries are
fetched by the user's machine with CrispASR's notices, not shipped by us.
Re-check on every CrispASR or model bump:
the pins, the release's notices, and that no GPL component entered the
prebuilt library.
