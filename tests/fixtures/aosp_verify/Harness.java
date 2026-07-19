import com.android.modules.utils.BinaryXmlPullParser;
import com.android.modules.utils.BinaryXmlSerializer;
import com.android.modules.utils.TypedXmlPullParser;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;

/**
 * Verifies `abx` (the Rust crate this file ships alongside) against the
 * real, unmodified AOSP {@code BinaryXmlSerializer}/{@code BinaryXmlPullParser}
 * by actually compiling and running them, not just reading the source.
 *
 * <p>Three things this does:
 * <ol>
 *   <li>Writes every {@code .abx} fixture in {@code tests/fixtures/} —
 *       built with the real serializer, using its real typed API
 *       ({@code attributeInt}, {@code attributeBoolean}, {@code attributeInterned},
 *       ...) rather than a plain-string-only translation, so they can be
 *       checked in and decoded by `abx`'s own test suite without needing a
 *       JDK present at normal {@code cargo test} time.
 *   <li>Re-checks the four findings recorded in this crate's CLAUDE.md
 *       "VERIFIED" section still hold, printing PASS/FAIL for each. If AOSP
 *       ever changes this behavior upstream, re-running this against a
 *       fresh checkout is how that would be caught.
 * </ol>
 *
 * <p>Run via the sibling {@code Containerfile} (needs podman/docker), or
 * directly with a JDK: see {@code README.md} in this repo for both.
 */
public class Harness {
    public static void main(String[] args) throws Exception {
        String outDir = args.length > 0 ? args[0] : ".";
        new File(outDir).mkdirs();

        write(outDir, "aosp_verify.abx", serializeMainDocument());
        write(outDir, "simple_pkg.abx", serializeSimplePkg());
        write(outDir, "nested_permissions.abx", serializeNestedPermissions());
        write(outDir, "booleans.abx", serializeBooleans());
        write(outDir, "repeated_strings.abx", serializeRepeatedStrings());
        write(outDir, "special_chars.abx", serializeSpecialChars());

        boolean ok = true;
        ok &= checkTypeNullTextBug();
        ok &= checkSignedHexRendering();
        ok &= checkPoolCapGracefulDegradation();

        if (!ok) {
            System.err.println("\nOne or more checks did not match what abx's CLAUDE.md documents. "
                    + "If AOSP's real behavior changed, abx's docs/code need a matching update.");
            System.exit(1);
        }
        System.out.println("\nAll checks matched abx's documented findings.");
    }

    static void write(String outDir, String name, byte[] data) throws Exception {
        try (FileOutputStream fos = new FileOutputStream(outDir + "/" + name)) {
            fos.write(data);
        }
        System.out.println("Wrote " + name + " (" + data.length + " bytes)");
    }

    /**
     * Every {@code AttributeValue} variant abx's encoder can produce
     * (excluding {@code attributeInterned}, which abx deliberately never
     * emits — see CLAUDE.md) and every text-bearing event type, plus a
     * repeated tag name to exercise interning back-references. Matches
     * {@code events_to_abx} on the Rust side byte-for-byte when fed the
     * equivalent {@code Event} stream.
     */
    static byte[] serializeMainDocument() throws Exception {
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        BinaryXmlSerializer ser = new BinaryXmlSerializer();
        ser.setOutput(buf, "UTF-8");
        ser.startDocument(null, null);
        ser.startTag(null, "root");
        ser.attribute(null, "str", "hello");
        ser.attributeBytesHex(null, "bh", new byte[]{(byte) 0xDE, (byte) 0xAD, (byte) 0xBE, (byte) 0xEF});
        ser.attributeBytesBase64(null, "bb", new byte[]{1, 2, 3});
        ser.attributeInt(null, "i", -42);
        ser.attributeIntHex(null, "ih", 0xCAFEBABE);
        ser.attributeLong(null, "l", -123456789012L);
        ser.attributeLongHex(null, "lh", 0xDEADBEEFCAFEBABEL);
        ser.attributeFloat(null, "f", 3.5f);
        ser.attributeDouble(null, "d", 2.71828d);
        ser.attributeBoolean(null, "bt", true);
        ser.attributeBoolean(null, "bf", false);
        ser.text("hello world");
        ser.cdsect("raw <not-a-tag>");
        ser.comment("a comment");
        ser.processingInstruction("pi target data");
        ser.entityRef("amp");
        ser.docdecl("some-decl");
        ser.ignorableWhitespace("   ");
        ser.text(""); // abx's AbxWriter always uses this form, never text(null) -- see checkTypeNullTextBug
        ser.startTag(null, "root");
        ser.endTag(null, "root");
        ser.endTag(null, "root");
        ser.endDocument();
        return buf.toByteArray();
    }

    /** Equivalent content to simple_pkg.xml, with version/flags properly
     * typed as ints (as real packages.xml-writing code would) rather than
     * left as plain strings. */
    static byte[] serializeSimplePkg() throws Exception {
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        BinaryXmlSerializer ser = new BinaryXmlSerializer();
        ser.setOutput(buf, "UTF-8");
        ser.startDocument(null, null);
        ser.startTag(null, "pkg");
        ser.attribute(null, "name", "com.example.chat");
        ser.attributeInt(null, "version", 3);
        ser.attributeInt(null, "flags", 1);
        ser.endTag(null, "pkg");
        ser.endDocument();
        return buf.toByteArray();
    }

    /** Equivalent content to nested_permissions.xml. All plain strings --
     * no typed values to choose here. */
    static byte[] serializeNestedPermissions() throws Exception {
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        BinaryXmlSerializer ser = new BinaryXmlSerializer();
        ser.setOutput(buf, "UTF-8");
        ser.startDocument(null, null);
        ser.startTag(null, "pkg");
        ser.attribute(null, "name", "com.example.chat");
        ser.startTag(null, "description");
        ser.text("A chat app");
        ser.endTag(null, "description");
        ser.startTag(null, "permission");
        ser.attribute(null, "name", "INTERNET");
        ser.endTag(null, "permission");
        ser.startTag(null, "permission");
        ser.attribute(null, "name", "CAMERA");
        ser.endTag(null, "permission");
        ser.endTag(null, "pkg");
        ser.endDocument();
        return buf.toByteArray();
    }

    /** Equivalent content to booleans.xml, with every attribute properly
     * typed (booleans via attributeBoolean, count/ratio via
     * attributeInt/attributeDouble) rather than left as plain strings. */
    static byte[] serializeBooleans() throws Exception {
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        BinaryXmlSerializer ser = new BinaryXmlSerializer();
        ser.setOutput(buf, "UTF-8");
        ser.startDocument(null, null);
        ser.startTag(null, "settings");
        ser.attributeBoolean(null, "enabled", true);
        ser.attributeBoolean(null, "hidden", false);
        ser.attributeInt(null, "count", 12345);
        ser.attributeDouble(null, "ratio", 3.14);
        ser.endTag(null, "settings");
        ser.endDocument();
        return buf.toByteArray();
    }

    /** Equivalent content to repeated_strings.xml. "id" is typed as int;
     * "category"/"name" use attributeInterned (not plain attribute) so
     * this still exercises value-interning back-references the way the
     * original fixture did -- real AOSP's attribute() never auto-interns a
     * value (see CLAUDE.md), only attributeInterned() does, so this is the
     * deliberate choice that keeps that coverage. */
    static byte[] serializeRepeatedStrings() throws Exception {
        String[][] items = {
                {"1", "tools", "Hammer"},
                {"2", "tools", "Wrench"},
                {"3", "tools", "Screwdriver"},
                {"4", "parts", "Bolt"},
                {"5", "parts", "Nut"},
                {"6", "parts", "Washer"},
                {"7", "tools", "Pliers"},
        };
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        BinaryXmlSerializer ser = new BinaryXmlSerializer();
        ser.setOutput(buf, "UTF-8");
        ser.startDocument(null, null);
        ser.startTag(null, "catalog");
        for (String[] item : items) {
            ser.startTag(null, "item");
            ser.attributeInt(null, "id", Integer.parseInt(item[0]));
            ser.attributeInterned(null, "category", item[1]);
            ser.attributeInterned(null, "name", item[2]);
            ser.endTag(null, "item");
        }
        ser.endTag(null, "catalog");
        ser.endDocument();
        return buf.toByteArray();
    }

    /** Equivalent content to special_chars.xml. Unlike xml2abx (which left
     * attribute-value entities raw/escaped -- a quirk of that independent
     * tool, not of AOSP), the "title" attribute here is the properly
     * *decoded* string: BinaryXmlSerializer.attribute() takes a plain Java
     * String with no XML-escaping concept at this API level, so a real
     * caller passes the actual decoded value. Text content is split into
     * explicit text()/entityRef() calls matching how a real XML-aware
     * caller built on this API would emit entities as distinct tokens
     * (this is what abx's own Event::EntityReference models). */
    static byte[] serializeSpecialChars() throws Exception {
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        BinaryXmlSerializer ser = new BinaryXmlSerializer();
        ser.setOutput(buf, "UTF-8");
        ser.startDocument(null, null);
        ser.startTag(null, "note");
        ser.attribute(null, "title", "Tom & Jerry <3>");
        ser.text("Use ");
        ser.entityRef("quot");
        ser.text("quotes");
        ser.entityRef("quot");
        ser.text(" ");
        ser.entityRef("amp");
        ser.text(" ");
        ser.entityRef("apos");
        ser.text("apostrophes");
        ser.entityRef("apos");
        ser.text(" safely");
        ser.endTag(null, "note");
        ser.endDocument();
        return buf.toByteArray();
    }

    /**
     * Finding 3 in CLAUDE.md: real {@code BinaryXmlPullParser} cannot
     * correctly read back a {@code TYPE_NULL} text token that real
     * {@code BinaryXmlSerializer} itself can produce via {@code text(null)}
     * -- {@code consumeToken()} calls {@code readUTF()} unconditionally for
     * text-bearing tokens, desyncing on a payload-less {@code TYPE_NULL}
     * token and silently truncating the rest of the document. This check
     * expects that bug to still reproduce; a PASS here confirms abx's
     * choice to never emit {@code TYPE_NULL} for text events is still the
     * only safe one.
     */
    static boolean checkTypeNullTextBug() throws Exception {
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        BinaryXmlSerializer ser = new BinaryXmlSerializer();
        ser.setOutput(buf, "UTF-8");
        ser.startDocument(null, null);
        ser.startTag(null, "a");
        ser.text(null);
        ser.endTag(null, "a");
        ser.endDocument();

        TypedXmlPullParser p = new BinaryXmlPullParser();
        p.setInput(new ByteArrayInputStream(buf.toByteArray()), "UTF-8");
        boolean sawEndTag = false;
        int type;
        while ((type = p.getEventType()) != TypedXmlPullParser.END_DOCUMENT) {
            if (type == TypedXmlPullParser.END_TAG) sawEndTag = true;
            p.nextToken();
        }
        // Expected (buggy) behavior: the parser desyncs after the TYPE_NULL
        // text token and never reports the EndTag -- it jumps straight to
        // END_DOCUMENT instead.
        boolean bugReproduced = !sawEndTag;
        report("TYPE_NULL text token still mishandled by real BinaryXmlPullParser", bugReproduced);
        return bugReproduced;
    }

    /**
     * Finding 4 in CLAUDE.md: {@code Attribute.getValueString()} for
     * {@code TYPE_INT_HEX}/{@code TYPE_LONG_HEX} uses
     * {@code Integer.toString(v, 16)}/{@code Long.toString(v, 16)}, which
     * treat {@code v} as signed -- a negative value renders as {@code -}
     * followed by the hex of its magnitude, not the raw bit pattern.
     */
    static boolean checkSignedHexRendering() {
        boolean ok = true;
        ok &= reportEquals("Integer.toString(0xCAFEBABE, 16)", "-35014542", Integer.toString(0xCAFEBABE, 16));
        ok &= reportEquals("Integer.toString(0x7FFFFFFF, 16)", "7fffffff", Integer.toString(0x7FFFFFFF, 16));
        ok &= reportEquals("Integer.toString(Integer.MIN_VALUE, 16)", "-80000000",
                Integer.toString(Integer.MIN_VALUE, 16));
        ok &= reportEquals("Long.toString(0xDEADBEEFCAFEBABEL, 16)", "-2152411035014542",
                Long.toString(0xDEADBEEFCAFEBABEL, 16));
        return ok;
    }

    /**
     * Also confirmed in CLAUDE.md: real {@code FastDataOutput.writeInternedUTF}
     * does not error past its 65,535-entry interning cap -- it silently
     * stops caching new entries (each written fresh instead) while entries
     * already interned keep back-referencing correctly.
     */
    static boolean checkPoolCapGracefulDegradation() {
        String label = "Real FastDataOutput stays exception-free past its 65535-entry interning cap";
        try {
            ByteArrayOutputStream buf = new ByteArrayOutputStream();
            BinaryXmlSerializer ser = new BinaryXmlSerializer();
            ser.setOutput(buf, "UTF-8");
            ser.startDocument(null, null);
            for (int i = 0; i < 65537; i++) {
                ser.startTag(null, "n" + i);
            }
            for (int i = 0; i < 65537; i++) {
                ser.endTag(null, "n" + i);
            }
            ser.endDocument();
            report(label, true);
            return true;
        } catch (Exception e) {
            report(label + " (threw " + e + ")", false);
            return false;
        }
    }

    static void report(String label, boolean pass) {
        System.out.println((pass ? "PASS: " : "FAIL: ") + label);
    }

    static boolean reportEquals(String label, String expected, String actual) {
        boolean pass = expected.equals(actual);
        report(label + " == " + expected + " (got " + actual + ")", pass);
        return pass;
    }
}
