#!/bin/bash
# 지은 Argo.app 을 가짜 서명해 IPA 로 묶는다.
#   package_ipa.sh <출력.ipa> <보일 이름> <빌드 번호>
set -e
OUT="$1"; DISPLAY="$2"; RUN="$3"

# DEPLOYMENT_LOCATION=YES 라 앱이 DerivedData 가 아니라 소스 옆에 놓인다. 둘 다 본다.
APP_PATH="$(find build/Build/Products Applications . -maxdepth 4 -name 'Argo.app' -type d 2>/dev/null | head -n 1)"
if [ -z "$APP_PATH" ]; then
  echo "Could not find Argo.app"
  find . -maxdepth 5 -name '*.app' -type d -print
  exit 1
fi
echo "App: $APP_PATH"

/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $RUN" "$APP_PATH/Info.plist" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleVersion string $RUN" "$APP_PATH/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $DISPLAY" "$APP_PATH/Info.plist" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string $DISPLAY" "$APP_PATH/Info.plist"

# TrollStore 에서 쓸 권한을 붙인다. 정보 파일을 고친 뒤에 서명한다.
ldid -S"$GITHUB_WORKSPACE/Argo.entitlements" "$APP_PATH/Argo"
find "$APP_PATH" -type f \( -name '*.dylib' -o -name 'RootHelper' \) -print0 \
  | xargs -0 -I{} ldid -S "{}" || true

rm -rf Payload "$OUT"
mkdir -p Payload
cp -R "$APP_PATH" Payload/
zip -qry "$OUT" Payload
rm -rf Payload
ls -lh "$OUT"

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_PATH/Info.plist" 2>/dev/null || echo 1.0)"
echo "IPA_VERSION=$VERSION" >> "$GITHUB_ENV"
