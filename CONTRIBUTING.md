# Contributing

```sh
cargo test
cargo run -- tests/fixtures/create-only.json
```

**Adding a rule:** create `src/rules/<rule_name>.rs` implementing `Rule`,
with tests for when it fires and when it doesn't, and register it in
`all_rules()` in `src/rules/mod.rs`.

**Adding a fixture:** see [tests/fixtures/README.md](tests/fixtures/README.md).

**Snapshot tests:** `tests/snapshots.rs` checks every fixture in every
output format against `tests/snapshots/`. When output changes on purpose,
accept it with `cargo insta review` (`cargo install cargo-insta`) or
`INSTA_UPDATE=always cargo test`, and commit the `.snap` files.

The design is in [PLAN.md](PLAN.md).
