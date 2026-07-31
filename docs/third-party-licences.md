# Third-party licences — the non-OSS components

The repository itself is Apache-2.0 (`LICENSE`, EXP-352) and the bulk of its
dependency graph is MIT/Apache/BSD/ISC, which the notice generator (EXP-375)
attributes mechanically. This file covers the handful of components a generator
**cannot** decide for us: source-available and closed-source code whose terms
have to be read, applied to our own entity, and re-checked over time.

Every determination below is dated. Re-open this file when headcount changes,
when a pinned version moves across a major boundary, or when a new distribution
surface appears.

## Remotion — source-available (EXP-380)

### What we depend on

`apps/marketing/package.json` pins `@remotion/cli`, `@remotion/google-fonts`,
`@remotion/player` and `remotion` at **4.0.484** — the ClosedLoop hero movie in
`apps/marketing/src/movie/`. `bun.lock` also resolves `@remotion/licensing`,
transitively via `@remotion/renderer`.

`remotion/package.json` declares `"license": "SEE LICENSE IN LICENSE.md"`, and
the published npm tarball **omits that file** (it ships `README.md`, `dist`,
`no-react.js`, `package.json`, `version.js`). The terms therefore live only
upstream, and are reproduced verbatim at
[`docs/licences/remotion-LICENSE.txt`](licences/remotion-LICENSE.txt) — fetched
2026-07-31 from `raw.githubusercontent.com/remotion-dev/remotion/main/LICENSE.md`,
sha256 `bd65083b940f61904f6ef298aade918a7cad72a3e35bc406e36fab365844b673`. It is
`.txt` rather than `.md` for the same reason the generated notices are: a
markdown formatter reflowing reproduced licence text is exactly what the licence
forbids, and nothing in the repo formats `.txt`.

### Determination 1 — which tier applies (2026-07-31)

**Free License. No Company Licence is required.**

The eligibility clause grants free use to, among others, "an individual" and "a
for-profit organization with up to 3 employees". Exponential is a one-person
operation, so it qualifies on both readings. There is no revenue threshold —
the test is purely headcount — and the Free License covers commercial use
explicitly.

**Re-check the moment the company reaches four people.** That is the only
trigger; nothing about traffic, revenue or how the movie is used moves us off
the free tier before then.

### Determination 2 — Remotion's compiled code in the exponential.at bundle (2026-07-31)

**Permitted. No change needed.**

`LoopMoviePlayer.tsx` imports `@remotion/player` and the movie surfaces import
`remotion` directly, so Remotion's compiled code sits in the lazy JS chunk
served to every visitor of exponential.at. That is the ordinary, documented use
of `@remotion/player`: embedding a Remotion composition in a web page cannot
happen without shipping the player to the browser, and the Free License's
allowed use is "to use the software non-commercially or commercially for the
purpose of creating videos and images".

The single disallowed use case is copying or modifying Remotion code "for the
purpose of selling, renting, licensing, relicensing, or sublicensing your own
derivate of Remotion". The marketing site sells Exponential; it neither
distributes Remotion as a product nor exposes its API to third parties.

Note also that the Free License imposes **no attribution requirement** — unlike
MIT/BSD it has no "must reproduce this notice" clause. Remotion's entry in the
marketing notices is our own audit trail, not a licence obligation.

### Determination 3 — Remotion inside the public `ghcr.io/niach/exponential-web` image (2026-07-31)

**Not appropriate. Stopped — see the `Dockerfile` runtime stage.**

The image's final stage re-ran `bun install --frozen-lockfile` over the whole
workspace, so Remotion (and every other marketing dependency) was installed into
the published image. Two problems, neither of which the Free License resolves:

1. **The licence is granted to us, on the basis of our entity size, and has no
   sublicence clause.** Nothing in it lets us pass rights along to whoever pulls
   the image. A self-hoster with more than three employees would end up holding
   Remotion's code with no licence covering them.
2. **The same image is the self-host distribution**, which people reasonably read
   as Apache-2.0 end to end. Shipping source-available code inside it — without
   even the `LICENSE.md`, which npm does not publish — misstates the terms.

There was never anything on the other side of the ledger: `apps/web` does not
import Remotion, the marketing site is a separate Vite build deployed separately
(`docs/deploys.md`), and the image runs neither. The runtime install is now
scoped with `--filter '@exp/web'`, which keeps `drizzle-kit`, `drizzle-orm` and
the workspace packages the boot-time migrate needs while dropping 376 packages —
verified in the pinned `oven/bun:1.3.10-alpine` base: 1415 → 1039 packages,
`node_modules` 1.0G → 770M, of which ~154M was the Remotion tree.

The builder stage deliberately still installs everything: it is discarded, only
`apps/web/.output` is copied out of it, and `@exp/widget`'s build reaches across
the workspace.

### Remotion 5.0 — a re-audit, not a bump

`LICENSE.md` opens by announcing that the licence changes in Remotion 5.0
(`remotion-dev/remotion#3750`). `apps/web/src/lib/third-party-licences.test.ts`
pins the recorded version against `apps/marketing/package.json` and fails on any
4.x → 5.x move, so the bump cannot land without someone re-reading the terms and
re-vendoring `docs/licences/remotion-LICENSE.txt`.

### Not a concern: `@remotion/google-fonts`

`apps/marketing/vite.config.ts` aliases `@remotion/google-fonts/Inter` and
`/JetBrainsMono` to local shims, so the movie never fetches from Google. The
package is installed for typechecking and the Remotion studio only; nothing of
it reaches the browser.

## Closed-source Google binaries — mobile only

Found in the same EXP-262 audit. All three are shipped inside the store builds,
none is open source, and none may appear in the MIT/Apache aggregate.

| Component | Client | Pulled in by | Terms |
| --- | --- | --- | --- |
| `GoogleAppMeasurement` | iOS | `firebase-ios-sdk` 11.15.0 (`apps/ios/Tuist/Package.resolved`) | Closed-source binary framework under Google's own terms — `github.com/google/GoogleAppMeasurement` ships no OSS licence for the binaries |
| `google-ads-on-device-conversion-ios-sdk` | iOS | `firebase-ios-sdk` 11.15.0 | Closed-source binary, Google Ads terms |
| `com.google.android.play:app-update-ktx` 2.1.0 | Android | `apps/android/gradle/libs.versions.toml` | Play Core Software Development Kit Terms of Service |

These are store-build-only and never reach the web app or the public image, so
there is no redistribution question of the kind Remotion raised — only a notices
question, answered below.

## The notices rule

Whatever the generator (EXP-375) emits, these components go in a dedicated
**"Commercially licensed components"** section of the per-client notices, and
**never** in the MIT/Apache aggregate — folding a source-available or
closed-source component into a list headed by a permissive licence asserts terms
that were never granted.

- **Marketing** (`apps/marketing/public/NOTICES.txt`) — Remotion, with the body
  of `docs/licences/remotion-LICENSE.txt` reproduced byte-for-byte. It is the
  only client that ships Remotion.
- **iOS** (`apps/ios/Exponential/Resources/NOTICES.txt`) — `GoogleAppMeasurement`
  and `google-ads-on-device-conversion-ios-sdk`, identified as closed-source
  Google binaries with a pointer to Google's terms (there is no licence body to
  reproduce).
- **Android** (`apps/android/app/src/main/assets/NOTICES.txt`) —
  `com.google.android.play:app-update-ktx`, under the Play Core SDK Terms of
  Service.
- **Web** (`apps/web/public/NOTICES.txt`) and **desktop** — nothing from this
  file. Both are Remotion-free by construction, and the section should be absent
  rather than empty.

This sits alongside, and follows the same rule as, EXP-375's trademarks section:
marks and non-OSS components each get their own heading, never an OSS one.
