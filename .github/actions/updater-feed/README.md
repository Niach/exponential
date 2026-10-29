# updater-feed

EXP-1430. The self-updater feed: `SHA256SUMS.txt` over a release dist dir plus
`SHA256SUMS.txt.minisig`, a minisign (ed25519) signature over its exact bytes.
Without it, whoever can publish a release asset can swap a binary AND its sums.

```yaml
- uses: Niach/exponential/.github/actions/updater-feed@<sha>   # this repo: ./repo/.github/actions/updater-feed
  with:
    dir: dist
    signing-key: ${{ secrets.UPDATE_SIGNING_KEY }}
    public-key: ${{ vars.UPDATE_SIGNING_PUBKEY }}
    # append: late.exe   # add/replace only these lines in an existing dist/SHA256SUMS.txt, re-sign
```

Linux runners only; minisign 0.12 is downloaded from the upstream release and
sha256-pinned in `feed.sh` (apt's 0.9 on 22.04 writes legacy signatures).

## Key

```bash
minisign -G -W -p update.pub -s update.key   # -W: unencrypted, CI has no prompt
```

| Where                                   | Gets                                                        |
| --------------------------------------- | ----------------------------------------------------------- |
| secret `UPDATE_SIGNING_KEY`             | `base64 -i update.key` (or the raw file)                    |
| variable `UPDATE_SIGNING_PUBKEY`        | the SECOND line of `update.pub` (bare base64)               |
| build env `EXP_UPDATE_PUBKEY`           | `${{ vars.UPDATE_SIGNING_PUBKEY }}` on the cargo build step |

Keep `update.key` offline after upload. Variable set + secret missing = the
action fails the release (every pinned client would refuse it); a public key
that does not match the secret fails too (the action verifies its own output).
Neither set = unsigned feed, one `::warning::`.

## Clients

`updater::pinned_pubkey()` reads `option_env!("EXP_UPDATE_PUBKEY")` at compile
time. Pinned: `updater::fetch_verified_sums` fetches both assets, checks the
signature over the raw sums bytes (prehashed ed25519 only, `minisign-verify`)
before any line is trusted, and refuses a missing or bad one. Unpinned (dev
builds): checksums only, as before.

One key per build, no multi-key pinning. Rotation = one transition release
compiled with the NEW public key but signed with the OLD secret (the action's
`public-key` = the old key for that run); after it, switch the secret.
