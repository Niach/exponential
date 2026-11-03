# @exp/store-shots

Dev-only picture tooling around exponential.at (once in
apps/marketing, now retired). Run from the repo root:

| command | what |
|---|---|
| `bun run screenshots:store` | App Store / Play Store compositor: the fastlane raws (`apps/{ios,android}/fastlane/screenshots-raw`) → the upload dirs (`scripts/store/`, flags in `render-store-screenshots.tsx`) |
| `bun run screenshots:pop-sidecars -- --platform ios\|android` | the pop-out rects the UI tests measured (`PopRects.swift` / `.kt`) → sidecars next to the raws |
| `bun run social:generate` | the X header + share cards → `apps/site/public/social/`, from the raw captures in `social-shots/` (`cd apps/web && bun run screenshots:social`) |
| `bun run --filter @exp/store-shots eapps:mocks` | the eApps concept pictures → `apps/site/public/eapps/` + `docs/spec/apps/` |

The Geist TTFs satori needs are the eUI conformance font set's
(`packages/exponential-ui/conformance/fonts/`).
Nothing in `out/` or `social-shots/` is committed.
