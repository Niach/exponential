# GitHub Actions

`build-issues-web.yml` builds the root `Dockerfile` and publishes
`ghcr.io/niach/exponential-web` on every push to `master` and on
`v*.*.*` / `v*.*.*-dev` tags.

The cloud at https://app.exponential.at runs a Coolify dockerimage
app that pulls this image. Coolify's control plane is home-LAN-only, so
**there is no auto-redeploy webhook**. After a green Actions run, deploy
manually from a LAN-connected machine:

```
coolify deploy uuid <issues-web-img-uuid>
```

Or click "Deploy" in the Coolify UI.

The home/Portainer pipeline at `.gitea/workflows/build-release.yml` is
unrelated and stays untouched.
