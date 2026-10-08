# Contributing

## Checks

Formatting, lints and commit messages are checked with
[pre-commit](https://pre-commit.com/) (`.pre-commit-config.yaml`): `cargo
fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and a
[Conventional Commits](https://www.conventionalcommits.org/) check. Install
the hooks after cloning:

```bash
pip install pre-commit
pre-commit install --hook-type pre-commit --hook-type commit-msg
```

`pre-commit run --all-files` runs all checks without committing.

## Tests

```bash
cargo test --all-features
```

- `tests/parser_tests.rs`, `tests/serde_tests.rs`: decoding, against bytes
  built with `tests/common/mod.rs`.
- `tests/aosp_fixture_tests.rs`, `tests/aosp_fixture_serde_tests.rs`:
  decoding files written by AOSP. A change to the wire format must pass
  these, not only the tests above. See
  [docs/AOSP_VERIFICATION.md](docs/AOSP_VERIFICATION.md) to regenerate them.
- `tests/encode_tests.rs`, `tests/xml_to_abx_tests.rs`: encoding.

Benchmarks are described in [docs/BENCHMARKS.md](docs/BENCHMARKS.md).
