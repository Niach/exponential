# cargo-fetch

EXP-1689: every job that runs cargo calls this after its toolchain and
rust-cache steps. It sets `CARGO_HTTP_MULTIPLEXING=false` and
`CARGO_NET_RETRY=10` for the rest of the job, then runs `cargo fetch --locked`
up to five times with a growing wait, so a crates.io blip never kills a long job.

| Input | Default | Meaning |
|---|---|---|
| `dir` | `apps/desktop` | The directory of the `Cargo.lock` to fetch. Empty = only set the network settings (a job with no lockfile to fetch, or one that only runs cargo-deny or cargo-fuzz). |
