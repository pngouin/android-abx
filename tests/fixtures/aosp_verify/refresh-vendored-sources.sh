#!/usr/bin/env sh
# Maintainer-only: re-fetch upstream AOSP sources into vendor/aosp/. Review the diff.
set -eu

cd "$(dirname "$0")"

PKG_DIR="vendor/aosp/com/android/modules/utils"
BASE_URL="https://android.googlesource.com/platform/frameworks/libs/modules-utils/+/refs/heads/main/java/com/android/modules/utils"

mkdir -p "$PKG_DIR"

for f in BinaryXmlSerializer.java BinaryXmlPullParser.java FastDataOutput.java \
         FastDataInput.java ModifiedUtf8.java TypedXmlSerializer.java TypedXmlPullParser.java; do
    echo "Fetching $f ..."
    curl -fsS "$BASE_URL/$f?format=TEXT" | base64 -d > "$PKG_DIR/$f"
done

echo "Done. Review with 'git diff vendor/aosp' before committing."
