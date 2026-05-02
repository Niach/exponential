#!/usr/bin/env bash
# One-time (idempotent) Garage bootstrap.
#
# After `bun run backend:up`, run `bun run storage:init` to:
#   1. Assign and apply a single-node cluster layout
#   2. Create the attachment bucket
#   3. Create an app access key and grant it read/write on the bucket
#   4. Print the access key + secret key for copying into .env
#
# Re-running is safe; each step is wrapped to skip if already applied.

set -euo pipefail

BUCKET="${S3_BUCKET:-exponential-attachments}"
KEY_NAME="exponential-app-key"
ZONE="dc1"
CAPACITY="1G"

g() { docker compose exec -T garage /garage "$@"; }

echo "==> Waiting for Garage to be reachable..."
for i in {1..30}; do
  if g status >/dev/null 2>&1; then break; fi
  sleep 1
done
g status >/dev/null

LAYOUT_JSON="$(g layout show --format json 2>/dev/null || echo '{}')"
if echo "$LAYOUT_JSON" | grep -q '"version": *0' || ! echo "$LAYOUT_JSON" | grep -q '"version"'; then
  echo "==> Assigning cluster layout..."
  NODE_ID="$(g node id -q | cut -d@ -f1)"
  g layout assign -z "$ZONE" -c "$CAPACITY" "$NODE_ID"
  g layout apply --version 1
else
  echo "==> Layout already applied, skipping."
fi

if g bucket info "$BUCKET" >/dev/null 2>&1; then
  echo "==> Bucket '$BUCKET' already exists, skipping."
else
  echo "==> Creating bucket '$BUCKET'..."
  g bucket create "$BUCKET"
fi

if g key info "$KEY_NAME" >/dev/null 2>&1; then
  echo "==> Key '$KEY_NAME' already exists, skipping create."
else
  echo "==> Creating key '$KEY_NAME'..."
  g key create "$KEY_NAME"
fi

echo "==> Granting key '$KEY_NAME' access to bucket '$BUCKET'..."
g bucket allow --read --write --owner "$BUCKET" --key "$KEY_NAME"

echo
echo "==> Done. Copy these values into your .env:"
echo
g key info "$KEY_NAME" --show-secret
