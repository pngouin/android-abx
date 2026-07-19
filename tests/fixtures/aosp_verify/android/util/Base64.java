package android.util;

/**
 * Minimal stand-in for the real {@code android.util.Base64} (part of the
 * Android SDK). {@code BinaryXmlPullParser} only ever calls
 * {@code encodeToString(byte[], NO_WRAP)}/{@code decode(String, NO_WRAP)},
 * which for the standard alphabet with padding is byte-identical to
 * {@code java.util.Base64}'s default encoder/decoder — this just forwards
 * to that. Original code, not copied from AOSP's real (much larger)
 * implementation.
 */
public class Base64 {
    public static final int NO_WRAP = 2;

    public static String encodeToString(byte[] input, int flags) {
        return java.util.Base64.getEncoder().encodeToString(input);
    }

    public static byte[] decode(String str, int flags) {
        return java.util.Base64.getDecoder().decode(str);
    }
}
