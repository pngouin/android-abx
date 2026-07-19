package android.text;

/**
 * Minimal stand-in for the real {@code android.text.TextUtils} (part of the
 * Android SDK). {@code BinaryXmlPullParser} only calls {@code isGraphic},
 * so only that method is provided here.
 *
 * <p>Unlike the other stubs in this directory, {@code isGraphic}'s body
 * below is copied verbatim (not reimplemented) from AOSP's real
 * {@code frameworks/base/core/java/android/text/TextUtils.java}, to avoid
 * introducing a behavioral difference of our own into a component this
 * verification harness is trying to test AOSP's real behavior through.
 * Source:
 * https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/text/TextUtils.java
 *
 * <p>Copyright (C) The Android Open Source Project, licensed under the
 * Apache License, Version 2.0 (http://www.apache.org/licenses/LICENSE-2.0).
 */
public class TextUtils {
    public static boolean isGraphic(CharSequence str) {
        final int len = str.length();
        for (int cp, i = 0; i < len; i += Character.charCount(cp)) {
            cp = Character.codePointAt(str, i);
            int gc = Character.getType(cp);
            if (gc != Character.CONTROL
                    && gc != Character.FORMAT
                    && gc != Character.SURROGATE
                    && gc != Character.UNASSIGNED
                    && gc != Character.LINE_SEPARATOR
                    && gc != Character.PARAGRAPH_SEPARATOR
                    && gc != Character.SPACE_SEPARATOR) {
                return true;
            }
        }
        return false;
    }
}
