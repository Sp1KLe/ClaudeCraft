#!/usr/bin/env bash
# Installs the APK on the emulator, launches it, and prints what happened.
# Never fails the job: the point is to read the log.
APK="$1"

echo "=== badging ==="
AAPT=$(ls "$ANDROID_HOME"/build-tools/*/aapt 2>/dev/null | tail -1)
PKG=$("$AAPT" dump badging "$APK" | sed -n "s/^package: name='\([^']*\)'.*/\1/p")
echo "package under test: $PKG"
"$AAPT" dump badging "$APK" 2>&1 | grep -E "^package|sdkVersion|targetSdk|native-code|launchable|uses-permission|application-label:" | head -20
echo "=== apk native libs ==="
unzip -l "$APK" | grep -E "\.so|\.dex" | head -20

echo "=== install ==="
adb install -r "$APK" 2>&1 | tail -3
adb logcat -c
echo "=== launch ==="
adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1 2>&1 | tail -3
sleep 25
echo "=== alive? ==="
adb shell pidof "$PKG" || echo "process is NOT running"

adb logcat -d > logcat.txt
echo "=== crash-related log lines ==="
grep -E "FATAL|AndroidRuntime|Fatal signal|SIGSEGV|SIGABRT|UnsatisfiedLink|panicked|miniquad|macroquad|claudecraft|libc  *:|DEBUG  *:|ActivityManager.*(Process|died|crash)" logcat.txt | head -120
echo "=== log tail ==="
tail -40 logcat.txt
exit 0
