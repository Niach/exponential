#!/usr/bin/env bash
#
# Create the App Store Connect record + Developer Portal App IDs for the iOS STAGING app
# (at.exponential.staging) — for TestFlight INTERNAL testing only. Run this ONCE.
#
# fastlane `produce` authenticates with your APPLE ID (username + 2FA); it does NOT accept
# the App Store Connect API key (that key is only for uploads: `fastlane beta_staging`).
# To do the 2FA a single time for the whole sequence, mint a session first:
#
#     fastlane spaceauth -u you@appleid.example      # interactive 2FA, prints FASTLANE_SESSION
#     export FASTLANE_SESSION='---BEGIN... the printed multiline value ...'
#     export FASTLANE_USER=you@appleid.example
#     apps/ios/scripts/create-staging-app.sh
#
# Without FASTLANE_SESSION each step re-prompts for 2FA — still works, just noisier.
# If a single step errors, you can do that one step by hand in the Developer portal
# (developer.apple.com/account → Identifiers) and re-run; the script is idempotent
# (produce no-ops when the id/group/app already exists).
set -euo pipefail

: "${FASTLANE_USER:?set FASTLANE_USER to your Apple ID email — see the header}"
TEAM="${APPLE_TEAM_ID:-V6W7BVCSM8}"          # DEVELOPMENT_TEAM in apps/ios/Project.swift
APP_BUNDLE="at.exponential.staging"
EXT_BUNDLE="at.exponential.staging.shareextension"
APP_GROUP="group.at.exponential.staging"     # matches ExponentialStaging.entitlements
APP_NAME="${STAGING_APP_NAME:-Exponential (Staging)}"

produce_cmd() { echo "+ produce $*"; fastlane produce "$@" -u "$FASTLANE_USER" -b "$TEAM"; }

echo "== 1/5 main App ID + ASC app record: ${APP_BUNDLE} (\"${APP_NAME}\") =="
produce_cmd create -a "$APP_BUNDLE" -q "$APP_NAME" -m en-US --skip_itc false

echo "== 2/5 capabilities on ${APP_BUNDLE}: Push Notifications + App Groups =="
produce_cmd enable_services -a "$APP_BUNDLE" --push-notification --app-group

echo "== 3/5 share-extension App ID (Developer portal only, no ASC app): ${EXT_BUNDLE} =="
produce_cmd create -a "$EXT_BUNDLE" -q "Exp Staging Share" -m en-US --skip_itc true
produce_cmd enable_services -a "$EXT_BUNDLE" --app-group

echo "== 4/5 App Group ${APP_GROUP} =="
produce_cmd group -g "$APP_GROUP" -n "Exponential Staging"

echo "== 5/5 associate both App IDs with ${APP_GROUP} =="
produce_cmd associate_group -a "$APP_BUNDLE" "$APP_GROUP"
produce_cmd associate_group -a "$EXT_BUNDLE" "$APP_GROUP"

echo
echo "Done: ASC record '${APP_NAME}' (${APP_BUNDLE}) created; App IDs + App Group registered."
echo "Next: cd apps/ios && fastlane beta_staging   # builds + uploads to TestFlight internal"
