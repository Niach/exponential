# Windows SmartScreen warning — options guide (FEED-4)

Decision guide only — nothing here is implemented yet (per the issue thread).

## What users are seeing and why

The Windows app ships as a raw, **unsigned** `Exponential-production-x86_64-windows.exe`
downloaded from GitHub Releases (see `.github/workflows/build-desktop.yml` — "a signed
MSI/MSIX is future work"). A browser download stamps the file with the Mark-of-the-Web;
on first launch Microsoft Defender SmartScreen sees an unknown, unsigned binary and shows
"Windows protected your PC" (users must click *More info → Run anyway*). Edge/Chrome may
additionally flag the download itself as "not commonly downloaded".

Two aggravating factors specific to our setup:

- **Reputation is per-file-hash when there is no signature.** Every release is a new hash,
  so whatever reputation a build earns dies with the next release. Unsigned = the warning
  effectively never goes away.
- The pain is concentrated on **first install**: our in-app updater downloads and
  `self-replace`s the exe itself (no browser, no Mark-of-the-Web), so updates don't
  re-trigger SmartScreen. Fixing the first-download experience fixes most of it.

**Do we need to sign the exe?** To make the warning go away reliably: yes — signing is the
only mechanism that lets reputation persist across releases (or, with the right issuer,
skip the reputation ramp entirely). An installer (MSI/MSIX) *without* a signature changes
nothing. The realistic question is which signing route, below.

## Options

### 0. Do nothing / document the bypass — free

Add a note next to the download link ("More info → Run anyway", plus the SHA256SUMS.txt we
already publish for verification). Zero cost, zero eligibility hurdles, but every release
warns every new user forever. Fine as a stopgap, not a fix.

### 1. SignPath Foundation (free code signing for OSS) — **free**

[signpath.org](https://signpath.org) sponsors code signing for open-source projects; the
repo is Apache-2.0 and public, so we should qualify. The certificate subject is
**"SignPath Foundation"** (not "Exponential"), which already carries accumulated
SmartScreen reputation from other OSS projects — typically no or short warning period.

Requirements/trade-offs:

- Signing runs through their SignPath service wired into CI (GitHub Actions integration
  exists); builds must come from CI, not laptops — ours do.
- Disclosure obligations: README/website must state binaries are signed by SignPath
  Foundation; they review the project (real OSS, release process sanity).
- The publisher shown in the UAC/properties dialog is SignPath Foundation, not us.
- Application/approval takes some back-and-forth; they can decline.

Best free option if the "signed by SignPath Foundation" publisher string is acceptable.

### 2. Azure Trusted Signing — **~$9.99/month** (Basic tier)

Microsoft's own signing service: short-lived certs under a Microsoft-operated CA, fully
managed keys, first-class `signtool`/GitHub Actions integration
(`azure/trusted-signing-action`). Because Microsoft itself vouches for the verified
identity, SmartScreen reputation is tied to the identity and in practice clears very
quickly — this is the best result-per-euro on the market.

The catch is **eligibility**, which has shifted repeatedly since launch: public-trust
identity validation has been limited to legal entities with ~3 years of verifiable
history, and individual-developer validation has been paused/limited at times. Whether we
qualify depends on what legal entity stands behind Exponential — verify the current rules
before planning around this.

### 3. Standard (OV) code-signing certificate — **~€70–300/year**

Classic route: buy an OV cert (cheap CAs: Certum — their *Open Source* cert is the
budget classic at roughly €70/yr and is available to individuals, shows the developer's
personal name as publisher; also SSL.com, Sectigo resellers like SignMyCode/KSoftware).
Since June 2023 CA/B rules require the private key in certified hardware, so everything is
a USB token or cloud-HSM signing service now — factor that into CI (cloud signing works
in GitHub Actions; a USB token does not).

Important caveat: **an OV signature does not remove the SmartScreen warning immediately.**
Reputation accrues to the certificate over weeks/enough downloads — but once earned it
*persists across releases*, which is the structural win over staying unsigned. Cheapest
self-owned-publisher option; slowest to show results.

### 4. EV code-signing certificate — **~$250–500+/year**

Extended Validation cert (requires a registered company; hardware/cloud key mandatory).
Historically EV meant *instant* SmartScreen reputation; Microsoft has since dropped that
official guarantee, though in practice EV still ramps much faster than OV. Given Trusted
Signing costs a fraction and is Microsoft-native, EV mainly makes sense if we want a
publisher-named cert *and* can't use Trusted Signing but do have a company.

### 5. Alternative distribution channels — free-to-$19, complements any of the above

These don't sign our exe but route around the browser-download + SmartScreen flow:

- **winget** (`microsoft/winget-pkgs` PR, free): supports "portable" exes; installs via
  winget skip the browser/MOTW flow, and the winget review pipeline does its own URL/
  malware screening. Cheap goodwill with exactly our dev audience. NB: some winget
  tooling frowns on unsigned installers — smoother after any signing option.
- **Scoop / Chocolatey** (free): same idea; Scoop is a natural fit for a portable exe and
  has no signing expectations.
- **Microsoft Store** ($19 one-time individual / $99 company): package as MSIX and the
  *Store* signs it — zero SmartScreen, ever. But it's a parallel SKU, not a fix for the
  website download: MSIX containerization conflicts with our `self-replace` updater and
  HKCU protocol/single-instance self-registration, so the Store build would need its own
  packaging + update path (Store-managed updates). Real work; only worth it if we want
  Store presence anyway.

### 6. Non-options (for completeness)

- **Unsigned MSI/MSIX installer**: identical warning, plus new packaging work. The
  workflow comment about "signed MSI/MSIX" is really about the *signed* part; a signed
  raw exe is equally fine and keeps the current updater working.
- Zipping the exe, download mirrors, etc.: MOTW propagates; no effect.

## Comparison

| Option | Cost | Publisher shown | Warning gone… | Blocker/risk |
|---|---|---|---|---|
| Do nothing | free | — | never | permanent first-run friction |
| SignPath Foundation | free | SignPath Foundation | ~immediately | approval + disclosure, not our name |
| Azure Trusted Signing | ~$10/mo | our verified identity | days-ish | eligibility (entity, 3y history) |
| OV cert (Certum OSS etc.) | ~€70+/yr | our name | weeks (then durable) | slow ramp; HSM/token logistics |
| EV cert | ~$300+/yr | company name | fast (not guaranteed) | needs a company; pricey |
| winget/Scoop | free | — | n/a (bypasses flow) | complement, not a fix |
| Microsoft Store MSIX | $19 once | Store-verified | immediately (Store only) | separate packaging + updater path |

## Recommendation

1. **Now, free:** apply to **SignPath Foundation** (we qualify on paper) and submit the
   exe to **winget/Scoop** in parallel.
2. **If a qualifying legal entity exists:** prefer **Azure Trusted Signing** (~$10/mo) —
   Microsoft-native, best SmartScreen outcome, our own publisher name.
3. **Fallback if neither works:** Certum-class OV cert (~€70/yr) and accept the
   reputation ramp.

Prices and eligibility rules in this space change often — re-verify before committing.

## Implementation notes for whichever route wins (future work)

- Sign in `build-desktop.yml` after `Build`, before the Windows artifact upload — the
  release asset **is** the self-update payload, so signing the released file covers both
  first install and updates. Sign staging too (staging self-updates).
- Always RFC 3161 **timestamp** signatures so they outlive cert expiry.
- Signing changes the file hash — it must happen **before** `SHA256SUMS.txt` is generated
  (it already is, if signing lives in the build job; keep it that way).
- The Windows matrix legs are `continue-on-error` — a broken signing step would silently
  drop the Windows asset from a release; add loud logging when that's wired up.
- The updater (`crates/updater`) swaps exes by content and doesn't verify signatures;
  once we sign, consider verifying the downloaded exe's signature before `self-replace`
  as a free integrity upgrade.
