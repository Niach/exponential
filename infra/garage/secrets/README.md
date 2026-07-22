# Garage secrets

Generate the two secret files locally before `bun run backend:up`:

```sh
openssl rand -hex 32 > rpc_secret
openssl rand -base64 32 > admin_token
chmod 600 rpc_secret admin_token   # garage refuses world-readable secret files
```

Both files are gitignored. The Garage container reads them via
`GARAGE_RPC_SECRET_FILE=/etc/garage/rpc_secret` and
`GARAGE_ADMIN_TOKEN_FILE=/etc/garage/admin_token` (mounted from this
directory).
