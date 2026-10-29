# apple-signing

Imports an iOS signing `.p12` (Apple Development + Apple Distribution certs
and keys) into an unlocked temp keychain on the user search list, so
`xcodebuild archive` / `-exportArchive` sign locally (EXP-1430). iOS-scoped:
the macOS app has its own import. Signing stays LOCAL on purpose (EXP-405:
Apple's cloud signer fails nested frameworks with error 90035).

## Inputs

| input | required | default | |
|---|---|---|---|
| `p12` | yes | | base64 of the `.p12` |
| `password` | yes | | the `.p12` password |
| `keychain-name` | no | `ci.keychain-db` | file name under `$RUNNER_TEMP` |

## Outputs

- `keychain-path`: the temp keychain (no post step: on a self-hosted Mac run
  `security delete-keychain <keychain-path>` when done).
- `identity`: SHA-1 of the Apple Distribution identity, else the first valid
  codesigning identity.

## Secrets

`IOS_DIST_CERT_P12` (base64 `.p12`), `IOS_DIST_CERT_PASSWORD`. The certs belong
to the Apple team, so a second app of the same team reuses them.

## From another repo

```yaml
- uses: Niach/exponential/.github/actions/apple-signing@<sha>
  id: signing
  with:
    p12: ${{ secrets.IOS_DIST_CERT_P12 }}
    password: ${{ secrets.IOS_DIST_CERT_PASSWORD }}
```
