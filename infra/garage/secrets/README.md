# Garage secrets

Generate the two secret files locally before `bun run backend:up`:

```sh
openssl rand -hex 32 > rpc_secret
openssl rand -base64 32 > admin_token
```

Both files are gitignored. The Garage container reads them via
`GARAGE_RPC_SECRET_FILE=/etc/garage/rpc_secret` and
`GARAGE_ADMIN_TOKEN_FILE=/etc/garage/admin_token` (mounted from this
directory).
