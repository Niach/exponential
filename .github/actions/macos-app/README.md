# macos-app

Assembles a macOS `.app` from built executable(s), codesigns it and packages
it (EXP-1430). Two or more `binaries` (one per arch slice) are `lipo -create`d
into a universal executable. With a Developer ID cert: hardened runtime +
secure timestamp, bundled dylibs signed with the app identity, then a
notarized + stapled `.dmg`. Without one: an ad-hoc-signed `.zip` and a
`::warning::` (keeps CI green before the Apple account). The scripts
(`bundle.sh`, `dmg.sh`) also run locally; Exponential's
`apps/desktop/scripts/build-macos-{app,dmg}.sh` delegate to them.

## Inputs

| input | required | default | |
|---|---|---|---|
| `binaries` | yes | | executable path(s), one per line; 2+ → universal |
| `app-name` | yes | | `<app-name>.app`, CFBundleName/DisplayName |
| `bundle-id` | yes | | CFBundleIdentifier |
| `executable` | yes | | Contents/MacOS name, CFBundleExecutable |
| `info-plist` | yes | | template; `${VERSION}`/`${BUILD}` substituted; URL schemes live here |
| `version` / `build` | no | `0.0.0` / `0` | CFBundleShortVersionString / CFBundleVersion |
| `entitlements` | no | | hardened-runtime entitlements |
| `icon` | no | | `.svg` (rasterized, librsvg installed if missing) or `.icns` |
| `frameworks-dir` | no | | its `*.dylib`/`*.framework` → Contents/Frameworks, signed |
| `resources` | no | | files/dirs → Contents/Resources (before signing), one per line |
| `lsuielement` | no | `false` | `true` = tray-only app (no Dock icon); `false` removes the key |
| `output-dir` | no | `dist` | where the `.app` and `.dmg`/`.zip` land |
| `dmg-name` / `zip-name` | no | `<app-name>.dmg` / `.zip` | artifact file names |
| `volume-name` | no | app name | `.dmg` volume name |
| `cert-p12`, `cert-password` | no | | base64 Developer ID Application `.p12` + password |
| `notary-key-id`, `notary-issuer-id`, `notary-key` | no | | App Store Connect API key (`notary-key` = base64 `.p8`) |

Outputs: `app`, `artifact` (the `.dmg` or `.zip`), `signed` (`true`/`false`).

## Secrets

`MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `NOTARY_KEY_ID`, `NOTARY_ISSUER_ID`,
`NOTARY_KEY`. The cert + key belong to the Apple team, so another app of the
same team reuses them. Cert set but notary absent → an un-notarized `.dmg`.

## From another repo

```yaml
- run: |
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
      rustup target add $t && cargo build --release --locked --target $t
    done
- uses: Niach/exponential/.github/actions/macos-app@<sha>
  with:
    binaries: |
      target/aarch64-apple-darwin/release/spezl
      target/x86_64-apple-darwin/release/spezl
    app-name: Spezl
    bundle-id: at.spezl.desktop
    executable: spezl
    info-plist: packaging/Info.plist
    entitlements: packaging/spezl.entitlements
    icon: assets/icon.svg
    lsuielement: "true"
    version: ${{ steps.ver.outputs.version }}
    build: ${{ github.run_number }}
    dmg-name: Spezl.dmg
    cert-p12: ${{ secrets.MACOS_CERT_P12 }}
    cert-password: ${{ secrets.MACOS_CERT_PASSWORD }}
    notary-key-id: ${{ secrets.NOTARY_KEY_ID }}
    notary-issuer-id: ${{ secrets.NOTARY_ISSUER_ID }}
    notary-key: ${{ secrets.NOTARY_KEY }}
```

Locally: the flags are documented at the top of `bundle.sh` and `dmg.sh`;
`MACOS_SIGN_IDENTITY` unset = ad-hoc.
