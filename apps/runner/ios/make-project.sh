#!/usr/bin/env bash
# #VAPP-268 (E6c): the iOS runner for ONE bundle. Emits an Xcode project
# (XcodeGen) that links RunnerKit, ExponentialUI and ONLY the declared
# capability modules (ITMS-90683: a linked privacy API needs its purpose
# string, so an undeclared one is never linked), with Info.plist from
# runner.json (bundle id, name, version, the deep-link scheme) and the
# purpose strings from the package's `capabilities[].why`; then optionally
# builds it with xcodebuild. E7's `exponential bundle --target ios` calls it,
# here or on a remote Mac, under the ios lane (scripts/lanes/runner-ios.sh).
#
#   bash apps/runner/ios/make-project.sh --bundle <app dir> --out <dir> \
#     [--modules ExponentialEnginePhotos,…] [--purposes <json>] [--icon <1024 png>] \
#     [--notices <file>] [--team <id> [--profile <name|uuid>] [--identity <cert>]] \
#     [--capabilities photos,files,share,print,notifications] [--build none|simulator|device]
#
# --bundle       the `app/` directory (runner.json + package.app.json), copied to <name>.app/app
# --modules      the capability modules to link (E7 passes bundle.json's
#                `manifest::ios_modules`); --capabilities photos,share is the
#                short form (photos → ExponentialEnginePhotos…); default = the
#                package manifest's scopes through packages/eengine/fixtures/bundle.json
#                `permissions.ios.modules` (THE one mapping, also in the kit)
# --purposes     a JSON object {Info.plist key: text} (E7 passes `manifest::ios_purposes`);
#                default = bundle.json `permissions.ios.purposes` of the linked
#                scopes, each the scope's `why`, else `purposeFallback`
# --icon         a 1024 px PNG (no alpha): the AppIcon asset catalog
# --notices      the third-party notices, shipped as <name>.app/NOTICES.txt
# --team         the DEVELOPMENT_TEAM written into the project (E7's [signing.ios]):
#                automatic signing, or manual with --profile (an installed App Store
#                provisioning profile) and --identity (default "Apple Distribution")
# --build        simulator = an unsigned iphonesimulator build; device = an
#                iphoneos build (signed with RUNNER_DEVELOPMENT_TEAM when set,
#                else unsigned for E7 to sign); none (default) = the project only
# Needs: Xcode 26+, xcodegen (brew), python3 (Xcode's), and the two
# xcframeworks with the slice you build for:
#   EENGINE_APPLE_TARGETS="aarch64-apple-ios-sim" bash apps/desktop/crates/eengine-ffi/build-ios.sh
#   EXPONENTIAL_UI_APPLE_TARGETS="aarch64-apple-ios-sim" bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh
# Prints the .app path last when it built one.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
ENGINE="$REPO/apps/desktop/crates/eengine-ffi"
EUI="$REPO/packages/exponential-ui-swift"

BUNDLE="" OUT="" CAPS="" BUILD="none" CAPS_SET=0 MODULES="" PURPOSES="" ICON="" NOTICES="" TEAM="" PROFILE="" IDENTITY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --bundle) BUNDLE="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --capabilities) CAPS="$2"; CAPS_SET=1; shift 2 ;;
    --build) BUILD="$2"; shift 2 ;;
    --modules) MODULES="$2"; shift 2 ;;
    --purposes) PURPOSES="$2"; shift 2 ;;
    --icon) ICON="$2"; shift 2 ;;
    --notices) NOTICES="$2"; shift 2 ;;
    --team) TEAM="$2"; shift 2 ;;
    --profile) PROFILE="$2"; shift 2 ;;
    --identity) IDENTITY="$2"; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$BUNDLE" ] && [ -n "$OUT" ] || { sed -n '11,15p' "$0" >&2; exit 2; }
CONTRACT="$REPO/packages/eengine/fixtures/bundle.json"
[ -f "$CONTRACT" ] || { echo "no $CONTRACT (the scope → module mapping)" >&2; exit 1; }
[ -z "$PURPOSES" ] || [ -f "$PURPOSES" ] || { echo "no --purposes file $PURPOSES" >&2; exit 1; }
[ -z "$ICON" ] || [ -f "$ICON" ] || { echo "no --icon file $ICON" >&2; exit 1; }
[ -z "$NOTICES" ] || [ -f "$NOTICES" ] || { echo "no --notices file $NOTICES" >&2; exit 1; }
case "$BUILD" in none|simulator|device) ;; *) echo "--build: none, simulator or device" >&2; exit 2 ;; esac
[ -f "$BUNDLE/runner.json" ] || { echo "no $BUNDLE/runner.json" >&2; exit 1; }
[ -f "$BUNDLE/package.app.json" ] || { echo "no $BUNDLE/package.app.json" >&2; exit 1; }
command -v xcodegen >/dev/null || { echo "xcodegen is missing (brew install xcodegen)" >&2; exit 1; }
for x in "$ENGINE/out/ExponentialEngineFFI.xcframework" "$EUI/Binaries/ExponentialUIFFI.xcframework"; do
  [ -d "$x" ] || { echo "missing $x (see the build-ios.sh lines in $0)" >&2; exit 1; }
done
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
BUNDLE="$(cd "$BUNDLE" && pwd)"

# runner.json + the manifest → the project spec, Info.plist and the capability glue.
python3 -I - "$BUNDLE" "$OUT" "$HERE" "$ENGINE" "$EUI" "$CAPS_SET" "$CAPS" "$CONTRACT" "$MODULES" "$PURPOSES" "$ICON" "$TEAM" "$PROFILE" "$IDENTITY" <<'PY'
import json, os, re, sys
bundle, out, here, engine, eui, caps_set, caps_arg, contract, modules_arg, purposes_arg, icon, team, profile, identity = sys.argv[1:15]
config = json.load(open(os.path.join(bundle, "runner.json")))
for key in ("id", "name", "version"):
    if not isinstance(config.get(key), str) or not config[key].strip():
        sys.exit(f"runner.json: {key} is missing (the runner would paint the config failure)")
file = json.load(open(os.path.join(bundle, "package.app.json")))
pkg = file.get("package", file) if isinstance(file, dict) else {}
manifest = {c.get("id"): c.get("why", "") for c in pkg.get("capabilities", []) or [] if isinstance(c, dict)}

# The capability modules: bundle.json `permissions.ios` is the ONE scope → module
# and scope → purpose mapping (Rust's eengine-bundle reads the same file).
ios = json.load(open(contract))["permissions"]["ios"]
SCOPES = ios["modules"]
KNOWN = sorted(set(SCOPES.values()))
if modules_arg:
    mods = [m.strip() for m in modules_arg.split(",") if m.strip()]
elif caps_set == "1":
    mods = ["ExponentialEngine" + c.strip().capitalize() for c in caps_arg.split(",") if c.strip()]
else:
    mods = [SCOPES[s] for s in manifest if s in SCOPES]
unknown = [m for m in mods if m not in KNOWN]
if unknown: sys.exit(f"unknown capability modules {unknown} (one of {KNOWN})")
mods = sorted(set(mods))

# Info.plist purpose strings = the author's `why` (only for what is linked).
if purposes_arg:
    purposes = json.load(open(purposes_arg))
    if not isinstance(purposes, dict) or not all(isinstance(v, str) for v in purposes.values()):
        sys.exit("--purposes: a JSON object of strings")
else:
    purposes = {}
    for scope, key in ios["purposes"].items():
        if SCOPES.get(scope) in mods:
            purposes[key] = manifest.get(scope) or ios["purposeFallback"][key].replace("{name}", config["name"])
# ITMS-90683 both ways: a linked privacy module needs its purpose string, an unlinked one never declares it.
for scope, key in ios["purposes"].items():
    if (SCOPES[scope] in mods) != (key in purposes):
        sys.exit(f"{key} and {SCOPES[scope]} disagree (linked: {SCOPES[scope] in mods}, declared: {key in purposes})")

name = config["name"]
target = re.sub(r"[^A-Za-z0-9]", "", name) or "Runner"
bundle_id = config["id"].replace("_", "-")  # a bundle id has no underscores; the URL scheme keeps the id
src = os.path.join(out, "Runner")
os.makedirs(src, exist_ok=True)

def esc(s): return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
extra = "".join(f"  <key>{k}</key>\n  <string>{esc(v)}</string>\n" for k, v in sorted(purposes.items()))
open(os.path.join(src, "Info.plist"), "w").write(f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!-- GENERATED by apps/runner/ios/make-project.sh from runner.json + the package's capabilities. -->
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>{esc(name)}</string>
  <key>CFBundleExecutable</key>
  <string>$(EXECUTABLE_NAME)</string>
  <key>CFBundleIdentifier</key>
  <string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>{esc(name)}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>{esc(config['version'])}</string>
  <key>CFBundleVersion</key>
  <string>{esc(config['version'])}</string>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key>
      <string>{esc(config['id'])}</string>
      <key>CFBundleURLSchemes</key>
      <array><string>{esc(config['id'])}</string></array>
    </dict>
  </array>
  <key>ITSAppUsesNonExemptEncryption</key>
  <false/>
  <key>LSRequiresIPhoneOS</key>
  <true/>
  <key>UILaunchScreen</key>
  <dict/>
  <key>UIApplicationSceneManifest</key>
  <dict>
    <key>UIApplicationSupportsMultipleScenes</key>
    <false/>
  </dict>
  <key>UISupportedInterfaceOrientations</key>
  <array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
  </array>
  <key>UISupportedInterfaceOrientations~ipad</key>
  <array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationPortraitUpsideDown</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
  </array>
{extra}</dict>
</plist>
""")

# The capability glue: ONLY the declared modules are imported and built.
ui = [m for m in mods if m != "ExponentialEngineNotifications"]
lines = ["// GENERATED by apps/runner/ios/make-project.sh: the declared capabilities only.",
         "import Foundation", "import ExponentialEngine"]
if ui: lines.append("import ExponentialEngineCapabilityCore")
lines += [f"import {m}" for m in mods]
notifications = "ExponentialEngineNotifications" in mods
if notifications: lines.append("import UserNotifications")
made = {"ExponentialEnginePhotos": "PhotosCapability(library: PhotoKitLibrary(), picker: SystemPhotoPicker(config))",
        "ExponentialEngineFiles": "FilesCapability(picker: SystemDocumentPicker(config))",
        "ExponentialEngineShare": "ShareCapability(sheet: SystemShareSheet(config))",
        "ExponentialEnginePrint": "PrintCapability(dialog: SystemPrintDialog(config))",
        "ExponentialEngineNotifications": "NotificationsCapability(scheduler: UserNotificationScheduler())"}
lines += ["", "enum RunnerCapabilities {",
          f"    static let declared: [String] = {json.dumps(mods)}", "",
          "    static func all() -> [Capability] {"]
if ui: lines.append("        let config = CapabilityConfig()")
lines.append("        return [" + ", ".join(made[m] for m in mods) + "]")
lines += ["    }", "", "    /// The tap of a notification the app posted → the instance (lifecycle.json `notificationTap`).",
          "    static func installTaps(_ handler: @escaping (String) -> Void) {"]
if notifications:
    lines += ["        TapDelegate.shared.handler = handler", "        UNUserNotificationCenter.current().delegate = TapDelegate.shared"]
lines += ["    }", "}"]
if notifications:
    lines += ["", "final class TapDelegate: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {",
              "    static let shared = TapDelegate()", "    var handler: ((String) -> Void)?", "",
              "    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification,",
              "                                withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {",
              "        completionHandler([.banner, .list, .sound])", "    }", "",
              "    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,",
              "                                withCompletionHandler completionHandler: @escaping () -> Void) {",
              "        if let tap = NotificationTaps.tapJson(response: response) { handler?(tap) }",
              "        completionHandler()", "    }", "}"]
open(os.path.join(src, "RunnerCapabilities.swift"), "w").write("\n".join(lines) + "\n")

# The AppIcon asset catalog (written below from --icon): one 1024 universal slot.
# Signing lives on the app target only (a command-line setting would reach the package targets too).
if team and profile:
    signing = (f"        CODE_SIGN_STYLE: Manual\n        DEVELOPMENT_TEAM: {team}\n"
               f"        PROVISIONING_PROFILE_SPECIFIER: \"{profile}\"\n        CODE_SIGN_IDENTITY: \"{identity or 'Apple Distribution'}\"\n")
elif team:
    signing = f"        CODE_SIGN_STYLE: Automatic\n        DEVELOPMENT_TEAM: {team}\n"
else:
    signing = "        CODE_SIGN_STYLE: Automatic\n"
icon_setting = "        ASSETCATALOG_COMPILER_APPICON_NAME: AppIcon\n" if icon else ""
deps = ["      - package: ExponentialEngine\n        product: ExponentialEngine"]
deps += [f"      - package: ExponentialEngine\n        product: {m}" for m in mods]
deps += ["      - package: ExponentialRunner\n        product: RunnerKit",
         "      - package: ExponentialUI\n        product: ExponentialUI",
         "      - package: ExponentialUI\n        product: ExponentialUIPrimitives"]
open(os.path.join(out, "project.yml"), "w").write(f"""# GENERATED by apps/runner/ios/make-project.sh ({config['id']} {config['version']}; modules: {', '.join(mods) or 'none'}).
name: {target}
options:
  deploymentTarget:
    iOS: "17.0"
  createIntermediateGroups: true
packages:
  ExponentialEngine:
    path: {engine}
  ExponentialUI:
    path: {eui}
  ExponentialRunner:
    path: {here}
targets:
  {target}:
    type: application
    platform: iOS
    sources:
      - path: Runner
        excludes: ["app/**", "app", "Info.plist"]
      - path: Runner/app
        type: folder
        buildPhase: resources
    settings:
      base:
        PRODUCT_NAME: {target}
        PRODUCT_BUNDLE_IDENTIFIER: {bundle_id}
        MARKETING_VERSION: {config['version']}
        CURRENT_PROJECT_VERSION: {config['version']}
        GENERATE_INFOPLIST_FILE: NO
        INFOPLIST_FILE: Runner/Info.plist
        TARGETED_DEVICE_FAMILY: "1,2"
        SWIFT_VERSION: "5.0"
        ENABLE_USER_SCRIPT_SANDBOXING: YES
        # No diagnostics in the runner: no debug symbols shipped, no testability.
        ENABLE_TESTABILITY: NO
        DEBUG_INFORMATION_FORMAT: dwarf
{signing}{icon_setting}    dependencies:
{chr(10).join(deps)}
""")
print(f"{target} {bundle_id} modules={','.join(mods) or 'none'} purposes={','.join(sorted(purposes)) or 'none'}")
PY

# The sources: the app template + the icon registry web draws, and the bundle at <name>.app/app.
# (Runner/Sources, never Runner/App: APFS is case-insensitive, `App` IS `app`.)
rm -rf "$OUT/Runner/app" "$OUT/Runner/Sources"
mkdir -p "$OUT/Runner/Sources"
cp "$HERE"/App/*.swift "$OUT/Runner/Sources/"
cp "$EUI/Example/KitchenSink/LucideIcons.swift" "$OUT/Runner/Sources/"
cp -R "$BUNDLE" "$OUT/Runner/app"
rm -rf "$OUT/Runner/Assets.xcassets" "$OUT/Runner/NOTICES.txt"
[ -z "$NOTICES" ] || cp "$NOTICES" "$OUT/Runner/NOTICES.txt"
if [ -n "$ICON" ]; then
  iconset="$OUT/Runner/Assets.xcassets/AppIcon.appiconset"
  mkdir -p "$iconset"
  cp "$ICON" "$iconset/AppIcon.png"
  printf '{"info":{"author":"xcode","version":1}}\n' >"$OUT/Runner/Assets.xcassets/Contents.json"
  printf '{"images":[{"filename":"AppIcon.png","idiom":"universal","platform":"ios","size":"1024x1024"}],"info":{"author":"xcode","version":1}}\n' >"$iconset/Contents.json"
fi
(cd "$OUT" && xcodegen generate --quiet --spec project.yml)
PROJECT="$(ls -d "$OUT"/*.xcodeproj | head -1)"
SCHEME="$(basename "$PROJECT" .xcodeproj)"
echo "project $PROJECT"
[ "$BUILD" = none ] && exit 0

DD="$OUT/DerivedData"
if [ "$BUILD" = simulator ]; then
  xcodebuild -project "$PROJECT" -scheme "$SCHEME" -configuration Release -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' \
    -derivedDataPath "$DD" ARCHS=arm64 CODE_SIGNING_ALLOWED=NO build -quiet
  APP="$DD/Build/Products/Release-iphonesimulator/$SCHEME.app"
else
  SIGN=(CODE_SIGNING_ALLOWED=NO)
  [ -n "${RUNNER_DEVELOPMENT_TEAM:-}" ] && SIGN=(DEVELOPMENT_TEAM="$RUNNER_DEVELOPMENT_TEAM" -allowProvisioningUpdates)
  xcodebuild -project "$PROJECT" -scheme "$SCHEME" -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
    -derivedDataPath "$DD" "${SIGN[@]}" build -quiet
  APP="$DD/Build/Products/Release-iphoneos/$SCHEME.app"
fi
[ -d "$APP/app" ] || { echo "the build has no $APP/app" >&2; exit 1; }
[ -z "$NOTICES" ] || [ -f "$APP/NOTICES.txt" ] || { echo "the build has no $APP/NOTICES.txt" >&2; exit 1; }
echo "$APP"
