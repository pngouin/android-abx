# Vendored third-party code

Everything under this `vendor/` directory is third-party code, checked in
verbatim so `tests/fixtures/aosp_verify/` builds fully offline (no network
access needed at build/run time). It is **not** covered by this repo's own
MIT license (see the top-level `LICENSE`/`README.md`) — each piece keeps
its own original license, as required by those licenses' own terms.

## `aosp/`

The real, unmodified source of `BinaryXmlSerializer.java`,
`BinaryXmlPullParser.java`, `FastDataOutput.java`, `FastDataInput.java`,
`ModifiedUtf8.java`, `TypedXmlSerializer.java`, and `TypedXmlPullParser.java`
— copied as-is (license headers intact) from AOSP's
`frameworks/libs/modules-utils`, `main` branch:
<https://android.googlesource.com/platform/frameworks/libs/modules-utils/+/refs/heads/main/java/com/android/modules/utils/>

Copyright (C) The Android Open Source Project. Licensed under the Apache
License, Version 2.0 (<http://www.apache.org/licenses/LICENSE-2.0>).

To refresh these against current upstream `main`, run
`../refresh-vendored-sources.sh` from this directory's parent (a manual,
maintainer-run tool — not invoked automatically by `build-and-run.sh` or
the `Containerfile`, both of which build fully offline from what's checked
in here).

## `xmlpull/`

`xmlpull-1.1.3.1.jar` from Maven Central
(`https://repo1.maven.org/maven2/xmlpull/xmlpull/1.1.3.1/`) — the standard
`org.xmlpull.v1` interfaces (`XmlPullParser`, `XmlSerializer`,
`XmlPullParserException`, `XmlPullParserFactory`) that AOSP's own
`BinaryXmlSerializer`/`BinaryXmlPullParser` implement. No sources jar is
published for this artifact, so the compiled classes are vendored directly
(a handful of small interfaces, ~7 KB total). Per the xmlpull project's own
declaration, this is public-domain software.
