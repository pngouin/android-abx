package android.annotation;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Minimal stand-in for the real {@code android.annotation.NonNull} (part of
 * the Android SDK, not available outside an Android build tree). Only
 * exists so the real, unmodified AOSP source below compiles; carries no
 * behavior.
 */
@Retention(RetentionPolicy.SOURCE)
@Target({ElementType.METHOD, ElementType.PARAMETER, ElementType.FIELD})
public @interface NonNull {}
