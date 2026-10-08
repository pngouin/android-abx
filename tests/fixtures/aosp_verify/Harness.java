import com.android.modules.utils.BinaryXmlPullParser;
import com.android.modules.utils.BinaryXmlSerializer;
import com.android.modules.utils.TypedXmlPullParser;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;

/** Writes the {@code .abx} fixtures with real AOSP and re-checks known AOSP behaviors. */
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

    /** Every encodable attribute type and text event, plus a repeated tag name. */
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
        ser.text("");
        ser.startTag(null, "root");
        ser.endTag(null, "root");
        ser.endTag(null, "root");
        ser.endDocument();
        return buf.toByteArray();
    }

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

    /** Uses attributeInterned to cover value back-references. */
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

    /** Attribute value passed decoded; entities as explicit entityRef() tokens. */
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

    /** Expects BinaryXmlPullParser to still misread a TYPE_NULL text token. */
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
        // Bug: parser skips the EndTag after the TYPE_NULL token.
        boolean bugReproduced = !sawEndTag;
        report("TYPE_NULL text token still mishandled by real BinaryXmlPullParser", bugReproduced);
        return bugReproduced;
    }

    /** Hex attributes render signed ({@code Integer.toString(v, 16)}). */
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

    /** writeInternedUTF stops caching past 65,535 entries without erroring. */
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
