# play-upload

Uploads a signed `.aab` to a Google Play track (EXP-1430). Wraps
`r0adkll/upload-google-play` (pinned by SHA; a node action, so it runs on
GitHub-hosted ubuntu and the Docker Mint runner without a Ruby toolchain) and
translates fastlane's `metadata/android/<locale>/changelogs/<versionCode>.txt`
into Play release notes.

Refuses the `production` track, an unsigned bundle and release notes over 500
characters. No service-account secret = a `::warning::` and the step is skipped
(`uploaded` = `false`).

## Inputs

| input | required | default | |
|---|---|---|---|
| `service-account-json` | no | `''` | Play service-account key, raw JSON or base64 |
| `package-name` | yes | | applicationId; the app record must already exist in the Play Console |
| `aab` | yes | | path to the signed bundle |
| `track` | no | `alpha` | `alpha` (default closed track), `beta`, `internal`, or a custom closed track |
| `status` | no | `completed` | `completed` or `draft` |
| `version-code` | with `changelog-dir` | `''` | picks `changelogs/<versionCode>.txt` |
| `changelog-dir` | no | `''` | e.g. `apps/android/fastlane/metadata/android` |
| `mapping-file` | no | `''` | R8 `mapping.txt` (skip it when the mapping rides the AAB) |

Output: `uploaded` (`true`/`false`).

## Secret

`PLAY_SERVICE_ACCOUNT_JSON`: a Google Cloud service-account JSON key. Play
Console → Users and permissions → invite the service account's email with
**Release manager** (or "Release to testing tracks") on the app. The Play API
cannot create apps: the app record and its first manual upload must exist.

## From another repo

```yaml
- uses: Niach/exponential/.github/actions/play-upload@<sha>
  with:
    service-account-json: ${{ secrets.PLAY_SERVICE_ACCOUNT_JSON }}
    package-name: app.spezl
    aab: app/build/outputs/bundle/release/app-release.aab
    track: alpha
    status: draft
    version-code: ${{ env.VERSION_CODE }}
    changelog-dir: fastlane/metadata/android
```
