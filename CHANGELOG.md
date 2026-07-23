# Changelog

All notable changes to this project are documented in this file, generated
automatically from [Conventional Commits](https://www.conventionalcommits.org/)
history by [git-cliff](https://git-cliff.org/).

## [0.2.1] - 2026-07-23

### Features

- *(decode)* Add AbxParser::write_xml for parity with AbxStreamParser


### Performance

- *(decode)* Stop AbxStreamParser from compacting on every event


### Documentation

- *(changelog)* Update CHANGELOG.md for v0.2.0

- *(readme)* Refresh Benchmarks section after streaming decode fix


### Miscellaneous Tasks

- *(release)* Bump version to 0.2.1


## [0.2.0] - 2026-07-23

### Bug Fixes

- Return Result<Option<T>> from find_attribute/attributes_of


### Refactor

- *(encode)* Dedupe write_utf into write_bytes_blob


### Documentation

- *(changelog)* Update CHANGELOG.md for v0.1.1


### Testing

- Drop redundant test_ prefix in parser_tests.rs


### Miscellaneous Tasks

- Add [lints] table to Cargo.toml

- *(release)* Bump version to 0.2.0


## [0.1.1] - 2026-07-23

### Features

- *(release)* Auto-generate CHANGELOG.md via git-cliff on each tag


### Styling

- Remove section-divider banner comments


### CI

- Add GitHub Actions workflow to test every commit and PR

- Harden CI workflow, add Dependabot config and status badges


### Miscellaneous Tasks

- *(deps)* Update serde to 1.0.229

- *(deps)* Update criterion 0.5 -> 0.8

- *(deps)* Update faster-hex 0.9 -> 0.10

- *(deps)* Update base64 0.22 -> 0.23

- *(deps)* Update thiserror 1 -> 2

- *(deps)* Update nom 7 -> 8

- *(release)* Bump version to 0.1.1


## [0.1.0] - 2026-07-23

### Features

- *(abx)* Add base implementation of the abx parser in stream and vec

- *(serde)* Add serde Deserialize support for ABX elements

- *(encode)* Add AbxWriter and events_to_abx low-level ABX encoder

- *(encode)* Add xml_to_abx XML-to-ABX encoding behind xml feature

- *(writer)* Reject too-long strings/byte blobs, matching AOSP's u16 length cap; stop tracking AI/editor config


### Bug Fixes

- *(stream)* Use nom streaming parsers for correct partial-read handling

- *(core)* Correct TYPE_* data-type nibble constants to match real AOSP

- *(event)* Render negative TYPE_INT_HEX/TYPE_LONG_HEX like real AOSP


### Performance

- *(bench)* Add criterion benchmarks for parsing and deserialization

- *(bench)* Add benchmarks for AbxWriter and xml_to_abx


### Refactor

- *(tests)* Extract ABX wire-format builders into shared module

- *(src)* Split decode/serde into submodules, extract wire.rs and event.rs


### Documentation

- *(lib)* Document ABX vs AXML as unrelated formats

- Add project guide and README

- Document encoder, AOSP verification, and fixture provenance

- Update README title to Android-ABX

- Document pre-commit setup in README


### Testing

- Add real xml2abx-encoded fixtures and AOSP regression tests

- Add reproducible AOSP verification harness

- Regenerate ABX fixtures from real AOSP, add byte-level oracle tests


### Styling

- Apply cargo fmt


### CI

- Add GitHub Actions workflow to publish to crates.io on tag push

- Create a GitHub release after a successful crates.io publish


### Miscellaneous Tasks

- Prepare crate for crates.io publication as android-abx

- Add repository/homepage/documentation to Cargo.toml

- Add pre-commit hooks for rustfmt, clippy, and Conventional Commits



