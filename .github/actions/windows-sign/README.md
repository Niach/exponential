# windows-sign

Authenticode-signs Windows files with Azure Artifact Signing (formerly
Trusted Signing; `azure/artifact-signing-action` v2.0.0, pinned by sha), SHA256
digest + Microsoft's RFC 3161 timestamp, then checks each with
`signtool verify /pa` (EXP-1430). No Azure credentials → a `::warning::` and
the files stay unsigned; a partial config fails the step. Sign BEFORE you hash
or upload the file.

## Inputs

| input | required | |
|---|---|---|
| `files` | yes | files to sign, one per line |
| `endpoint` | for signing | regional endpoint, e.g. `https://weu.codesigning.azure.net/` |
| `account-name` | for signing | the Artifact Signing account |
| `certificate-profile` | for signing | the certificate profile |
| `azure-tenant-id`, `azure-client-id`, `azure-client-secret` | for signing | the app registration (role "Artifact Signing Certificate Profile Signer") |

Output: `signed` (`true`/`false`). Runs on a Windows runner.

## Secrets + variables

Secrets `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`; repo
variables `WINDOWS_SIGNING_ENDPOINT`, `WINDOWS_SIGNING_ACCOUNT`,
`WINDOWS_SIGNING_PROFILE`. One account + profile can sign every app of the
same publisher.

## From another repo

```yaml
- uses: Niach/exponential/.github/actions/windows-sign@<sha>
  with:
    files: target/release/spezl.exe
    endpoint: ${{ vars.WINDOWS_SIGNING_ENDPOINT }}
    account-name: ${{ vars.WINDOWS_SIGNING_ACCOUNT }}
    certificate-profile: ${{ vars.WINDOWS_SIGNING_PROFILE }}
    azure-tenant-id: ${{ secrets.AZURE_TENANT_ID }}
    azure-client-id: ${{ secrets.AZURE_CLIENT_ID }}
    azure-client-secret: ${{ secrets.AZURE_CLIENT_SECRET }}
```
