package android.util;

/** Stub of {@code android.util.Base64}, forwards to {@code java.util.Base64} (NO_WRAP). */
public class Base64 {
    public static final int NO_WRAP = 2;

    public static String encodeToString(byte[] input, int flags) {
        return java.util.Base64.getEncoder().encodeToString(input);
    }

    public static byte[] decode(String str, int flags) {
        return java.util.Base64.getDecoder().decode(str);
    }
}
