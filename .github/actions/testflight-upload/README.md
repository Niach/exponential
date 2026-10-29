# testflight-upload

Uploads an exported App Store `.ipa` to TestFlight with `fastlane pilot` and an
App Store Connect API key (EXP-1430). macOS runner; fastlane is on the GitHub
macOS images and gem-installed otherwise. Internal by default: no processing
wait, no external group. "Submit for Review" stays manual in App Store Connect.

## Inputs

| input | required | default | |
|---|---|---|---|
| `ipa` | yes | | path to the exported `.ipa` |
| `asc-key-id` | yes | | ASC API key id |
| `asc-issuer-id` | yes | | ASC API issuer id |
| `asc-key-content` | yes | | the `.p8`, raw PEM or base64-wrapped PEM |
| `distribute-external` | no | `false` | `true` waits for processing, assigns `groups`, submits for Beta App Review |
| `groups` | with external | `''` | external tester group(s), comma-separated |
| `changelog` | with external | `''` | What-to-Test text |

## Secrets

`ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_CONTENT`: App Store Connect → Users and
Access → Integrations → Team Keys, role **App Manager**. One team key serves
every app of the team.

## From another repo

```yaml
- uses: Niach/exponential/.github/actions/testflight-upload@<sha>
  with:
    ipa: ${{ runner.temp }}/export/Spezl.ipa
    asc-key-id: ${{ secrets.ASC_KEY_ID }}
    asc-issuer-id: ${{ secrets.ASC_ISSUER_ID }}
    asc-key-content: ${{ secrets.ASC_KEY_CONTENT }}
```
