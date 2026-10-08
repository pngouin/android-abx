# AOSP verification

The `.abx` files in `tests/fixtures/` are written by AOSP's
`BinaryXmlSerializer`, compiled from source vendored in
`tests/fixtures/aosp_verify/vendor/`. They are decoded on every test run by
`tests/aosp_fixture_tests.rs` and `tests/aosp_fixture_serde_tests.rs`.

The tests in `tests/parser_tests.rs` and `tests/serde_tests.rs` use byte
builders written for this crate. They check internal consistency, not
compatibility: an early version of this crate had every type nibble off by
one and those tests still passed. Only the AOSP fixtures caught it.

## What is checked

- Decoding: every fixture decodes to the expected events and values.
- Encoding: a document covering every `AttributeValue` variant, every text
  event and repeated names encodes byte for byte the same as
  `BinaryXmlSerializer`.
- Behaviour copied from AOSP:
  - Hex-typed values (`IntHex`, `LongHex`) render as signed hex, like
    `Integer.toString(v, 16)`: `0xCAFEBABE` renders as `-35014542`.
  - Past 65,535 unique names, the writer stops interning new names
    instead of failing, like `FastDataOutput.writeInternedUTF`.
  - Text events are never written as `TYPE_NULL`. `BinaryXmlPullParser`
    reads a `TYPE_NULL` text token as if it had a payload and silently
    drops the rest of the document. This is a bug in AOSP.

## Re-running

The harness compiles AOSP's serializer and parser, writes the fixtures and
prints PASS or FAIL for each check. The sources and their one dependency
(`xmlpull`) are vendored, so it runs offline. Licenses are listed in
`vendor/NOTICE.md`.

With a JDK on `PATH`:

```bash
cd tests/fixtures/aosp_verify
./build-and-run.sh aosp_verify.abx
```

With podman or docker (the checks run during the build, so a failed build
means a failed check):

```bash
cd tests/fixtures/aosp_verify
podman build -t abx-aosp-verify -f Containerfile .
podman create --name abx-aosp-verify-tmp abx-aosp-verify
podman cp abx-aosp-verify-tmp:/work/aosp_verify.abx .
podman rm abx-aosp-verify-tmp
```

`refresh-vendored-sources.sh` fetches the current AOSP `main` sources into
`vendor/aosp/`. Review the diff before committing it.
