#!/usr/bin/env bash
# Arma target/aab/out/app.aab (para subir a Play) y universal.apk (para probar por adb).
# Requisitos: Android SDK (build-tools 35, platforms 36), NDK 27, bundletool, JDK, ImageMagick, secret-tool.
# Uso: tools/build-aab.sh            (VERSION_CODE=3 VERSION_NAME=0.1.2 tools/build-aab.sh para una nueva versión)
set -euo pipefail
cd "$(dirname "$0")/.."

PKG=io.github.twint.lgdoom
APP_NAME="Zap Remote"
VERSION_CODE=${VERSION_CODE:-2}
VERSION_NAME=${VERSION_NAME:-0.1.1}
MIN_SDK=26
TARGET_SDK=36
LIB=lg_app

ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk}
NDK=${ANDROID_NDK_ROOT:-$ANDROID_HOME/ndk/27.2.12479018}
BT=$ANDROID_HOME/build-tools/35.0.0
JAR=$ANDROID_HOME/platforms/android-$TARGET_SDK/android.jar
BUNDLETOOL=${BUNDLETOOL:-$HOME/Android/tools/bundletool.jar}
KS=${KS:-$HOME/.local/share/lg-remote/upload.jks}
KEY_ALIAS=upload
TC=$NDK/toolchains/llvm/prebuilt/linux-x86_64
OUT=target/aab
PKGDIR=$OUT/pkg

# --- 1) librería nativa: enlaza contra API $MIN_SDK (no contra el target) y páginas de 16 KB (exigido por Play) ---
export CC_aarch64_linux_android=$TC/bin/aarch64-linux-android$MIN_SDK-clang
export CXX_aarch64_linux_android=$TC/bin/aarch64-linux-android$MIN_SDK-clang++
export AR_aarch64_linux_android=$TC/bin/llvm-ar
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$CC_aarch64_linux_android
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$TC/sysroot --target=aarch64-linux-android$MIN_SDK"
export ANDROID_HOME
cargo build --release --lib -p lg-app --target aarch64-linux-android --target-dir $OUT
SO=$OUT/aarch64-linux-android/release/lib$LIB.so

# --- 2) recursos y manifest ---
rm -rf $PKGDIR && mkdir -p $PKGDIR/res/values $PKGDIR/res/mipmap-anydpi-v26 $PKGDIR/res/drawable-xxxhdpi $OUT/out
for d in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  mkdir -p $PKGDIR/res/mipmap-${d%%:*}
  magick icon/icon.png -resize ${d##*:}x${d##*:} $PKGDIR/res/mipmap-${d%%:*}/ic_launcher.png
done
cp icon/adaptive-fg-432.png $PKGDIR/res/drawable-xxxhdpi/ic_launcher_foreground.png
cat > $PKGDIR/res/values/strings.xml <<EOF
<resources><string name="app_name">$APP_NAME</string><color name="ic_launcher_background">$(cat icon/adaptive-bg.txt)</color></resources>
EOF
cat > $PKGDIR/res/mipmap-anydpi-v26/ic_launcher.xml <<'EOF'
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
  <background android:drawable="@color/ic_launcher_background"/>
  <foreground android:drawable="@drawable/ic_launcher_foreground"/>
</adaptive-icon>
EOF
cat > $PKGDIR/AndroidManifest.xml <<EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="$PKG">
  <uses-permission android:name="android.permission.INTERNET"/>
  <application android:label="@string/app_name" android:icon="@mipmap/ic_launcher" android:hasCode="false" android:allowBackup="false">
    <activity android:name="android.app.NativeActivity" android:exported="true" android:launchMode="singleTop"
        android:hardwareAccelerated="true" android:windowSoftInputMode="adjustResize"
        android:configChanges="locale|keyboard|keyboardHidden|orientation|screenLayout|uiMode|screenSize|smallestScreenSize|density|layoutDirection|fontScale">
      <meta-data android:name="android.app.lib_name" android:value="$LIB"/>
      <intent-filter>
        <action android:name="android.intent.action.MAIN"/>
        <category android:name="android.intent.category.LAUNCHER"/>
      </intent-filter>
    </activity>
  </application>
</manifest>
EOF
$BT/aapt2 compile --dir $PKGDIR/res -o $PKGDIR/res.zip
$BT/aapt2 link --proto-format -o $PKGDIR/linked.zip -I "$JAR" --manifest $PKGDIR/AndroidManifest.xml \
  --min-sdk-version $MIN_SDK --target-sdk-version $TARGET_SDK --version-code "$VERSION_CODE" --version-name "$VERSION_NAME" \
  -R $PKGDIR/res.zip --auto-add-overlay

# --- 3) módulo base -> AAB ---
MOD=$PKGDIR/base
mkdir -p $MOD/manifest $MOD/lib/arm64-v8a
unzip -q -o $PKGDIR/linked.zip -d $PKGDIR/linked
mv $PKGDIR/linked/AndroidManifest.xml $MOD/manifest/AndroidManifest.xml
cp -r $PKGDIR/linked/res $PKGDIR/linked/resources.pb $MOD/
cp $SO $MOD/lib/arm64-v8a/
(cd $MOD && rm -f ../base.zip && zip -q -r ../base.zip .)
rm -f $OUT/out/app.aab
java -jar "$BUNDLETOOL" build-bundle --modules=$PKGDIR/base.zip --output=$OUT/out/app.aab

# --- 4) keystore de subida (se crea una vez; la clave queda en el keyring) ---
PASS=$(secret-tool lookup name lgdoom_upload_keystore 2>/dev/null || true)
if [ -z "$PASS" ]; then
  PASS=$(openssl rand -base64 24 | tr -d '/+=')
  printf %s "$PASS" | secret-tool store --label="$APP_NAME upload keystore" name lgdoom_upload_keystore
fi
if [ ! -f "$KS" ]; then
  mkdir -p "$(dirname "$KS")"
  keytool -genkeypair -keystore "$KS" -alias $KEY_ALIAS -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=$APP_NAME upload" -storepass "$PASS" -keypass "$PASS" 2>/dev/null
  chmod 600 "$KS"
  echo ">> Keystore nuevo en $KS: hacé una copia de seguridad (con la clave del keyring 'lgdoom_upload_keystore')."
fi
PASSFILE=$(mktemp); trap 'rm -f "$PASSFILE"' EXIT; chmod 600 "$PASSFILE"; printf %s "$PASS" > "$PASSFILE"
jarsigner -keystore "$KS" -storepass:file "$PASSFILE" -sigalg SHA256withRSA -digestalg SHA-256 $OUT/out/app.aab $KEY_ALIAS >/dev/null

# --- 5) APK universal firmado con la misma clave, para probar en el celular ---
rm -f $OUT/out/app.apks
java -jar "$BUNDLETOOL" build-apks --bundle=$OUT/out/app.aab --output=$OUT/out/app.apks --mode=universal \
  --ks="$KS" --ks-pass=file:"$PASSFILE" --ks-key-alias=$KEY_ALIAS --key-pass=file:"$PASSFILE"
unzip -p $OUT/out/app.apks universal.apk > $OUT/out/universal.apk
ls -l $OUT/out
