#!/bin/bash
# KasaDesk.app 굽기 — dist/KasaDesk.app. 서명은 환경으로 정한다:
#   KASADESK_SIGN_ID       서명 인증서(sha1 또는 이름). 없으면 ad-hoc.
#   KASADESK_SIGN_KEYCHAIN 그 인증서가 든 열쇠고리.
#   KASADESK_SIGN_UNLOCK   서명 바로 앞에 부를 열쇠고리 풀기 실행 파일(긴 컴파일 동안 다시 잠기는 것을 막는다).
#   KASADESK_HARDENED=1    공증용 — hardened runtime·보안 타임스탬프, 서명 실패를 삼키지 않는다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SPARKLE_VER="2.9.3"
FEED="https://2rami.github.io/kasadesk/appcast.xml"
ED_KEY="$(sed -n 's/^pub const ED_PUBLIC_KEY: &str = "\(.*\)";$/\1/p' src/update.rs)"
VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
[[ -n "$ED_KEY" && -n "$VERSION" ]] || { echo "error: 판 번호나 피드 공개키를 못 읽었다" >&2; exit 1; }

git -C kasaterm lfs pull --include="crates/kasa-gridview/assets/fonts/*" >/dev/null 2>&1 || true
head -c 7 kasaterm/crates/kasa-gridview/assets/fonts/D2Coding-Regular.ttf | grep -q "^version" && {
  echo "error: 카사텀 서브모듈 글꼴이 LFS 포인터다 — git -C kasaterm lfs pull" >&2; exit 1; }

cargo build --locked --release
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/kasadesk"

FW_SRC="vendor/Sparkle/Sparkle.framework"
if [[ ! -d "$FW_SRC" ]]; then
  mkdir -p vendor/Sparkle
  curl -sSLf -o "vendor/Sparkle-$SPARKLE_VER.tar.xz" \
    "https://github.com/sparkle-project/Sparkle/releases/download/$SPARKLE_VER/Sparkle-$SPARKLE_VER.tar.xz"
  tar -xJf "vendor/Sparkle-$SPARKLE_VER.tar.xz" -C vendor/Sparkle/
fi

APP="dist/KasaDesk.app"
rm -rf "${APP:?}"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp "$BIN" "$APP/Contents/MacOS/kasadesk"
cp assets/DeskIcon.icns "$APP/Contents/Resources/DeskIcon.icns"
ditto "$FW_SRC" "$APP/Contents/Frameworks/Sparkle.framework"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.kasa.kasadesk</string>
    <key>CFBundleName</key>
    <string>KasaDesk</string>
    <key>CFBundleDisplayName</key>
    <string>KasaDesk</string>
    <key>CFBundleExecutable</key>
    <string>kasadesk</string>
    <key>CFBundleIconFile</key>
    <string>DeskIcon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>NSRequiresAquaSystemAppearance</key>
    <false/>
    <key>NSLocalNetworkUsageDescription</key>
    <string>같은 망에 있는 기기에 바로 붙어 화면을 받아요.</string>
    <key>SUFeedURL</key>
    <string>$FEED</string>
    <key>SUPublicEDKey</key>
    <string>$ED_KEY</string>
    <key>SUEnableAutomaticChecks</key>
    <true/>
    <key>SUAutomaticallyUpdate</key>
    <true/>
    <key>SUScheduledCheckInterval</key>
    <integer>3600</integer>
</dict>
</plist>
PLIST

SIGN="${KASADESK_SIGN_ID:--}"
ARGS=(--force --sign "$SIGN")
[[ -n "${KASADESK_SIGN_KEYCHAIN:-}" ]] && ARGS+=(--keychain "$KASADESK_SIGN_KEYCHAIN")
HARDENED="${KASADESK_HARDENED:-0}"
if [[ "$HARDENED" == "1" ]]; then
  [[ "$SIGN" != "-" ]] || { echo "error: KASADESK_HARDENED=1 에는 Developer ID 인증서가 필요하다" >&2; exit 1; }
  ARGS+=(--options runtime --timestamp)
  [[ -n "${KASADESK_SIGN_UNLOCK:-}" ]] && "$KASADESK_SIGN_UNLOCK"
fi
sign() {
  if [[ "$HARDENED" == "1" ]]; then codesign "${ARGS[@]}" "$1"; else codesign "${ARGS[@]}" "$1" 2>/dev/null || true; fi
}
# Sparkle 은 안쪽(XPC·Updater·Autoupdate)부터 → 프레임워크 → 앱 순으로 서명한다. --deep 한 방은 안쪽 XPC 서명이 어긋날 수 있다.
FW="$APP/Contents/Frameworks/Sparkle.framework"
for nested in "$FW/Versions/B/XPCServices/Downloader.xpc" "$FW/Versions/B/XPCServices/Installer.xpc" \
              "$FW/Versions/B/Updater.app" "$FW/Versions/B/Autoupdate"; do
  [[ -e "$nested" ]] && sign "$nested"
done
sign "$FW"
sign "$APP"
echo "[bundle-mac] $APP $VERSION ($([[ "$SIGN" == "-" ]] && echo ad-hoc || echo signed))"
