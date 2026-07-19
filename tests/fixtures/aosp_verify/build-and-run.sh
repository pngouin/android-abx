#!/usr/bin/env sh
# Compiles this directory's stubs, the vendored real AOSP source
# (vendor/aosp/), and Harness.java, then runs the harness. Fully offline --
# everything needed is already checked into vendor/ (see vendor/NOTICE.md).
# See README.md ("Verifying against real AOSP") for what this checks and why.
set -eu

cd "$(dirname "$0")"

XMLPULL_JAR="vendor/xmlpull/xmlpull-1.1.3.1.jar"
AOSP_SRC="vendor/aosp/com/android/modules/utils"

mkdir -p out
javac -d out -cp "$XMLPULL_JAR" \
    android/annotation/NonNull.java \
    android/annotation/Nullable.java \
    android/text/TextUtils.java \
    android/util/Base64.java \
    "$AOSP_SRC/ModifiedUtf8.java" \
    "$AOSP_SRC/FastDataOutput.java" \
    "$AOSP_SRC/FastDataInput.java" \
    "$AOSP_SRC/TypedXmlSerializer.java" \
    "$AOSP_SRC/TypedXmlPullParser.java" \
    "$AOSP_SRC/BinaryXmlSerializer.java" \
    "$AOSP_SRC/BinaryXmlPullParser.java" \
    Harness.java

OUT_DIR="${1:-.}"
java -cp "out:$XMLPULL_JAR" Harness "$OUT_DIR"
