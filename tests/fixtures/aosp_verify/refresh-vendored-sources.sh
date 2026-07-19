#!/usr/bin/env sh
# Maintainer-only: re-fetches current upstream AOSP source and overwrites
# the copies checked into vendor/aosp/, so they can be periodically synced
# with real AOSP `main`. Not run automatically by build-and-run.sh or the
# Containerfile -- both build fully offline from what's already vendored.
# Review the diff before committing; a behavior change upstream is exactly
# what this whole harness exists to catch.
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
