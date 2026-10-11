# planrisk

Reads a Terraform plan and tells a reviewer what is risky about it: what
gets destroyed, what gets replaced and why, what gets opened up, and how
much changes at once. Built to drop into CI as a single binary.

**Status:** in development, not yet released. See [PLAN.md](PLAN.md) for
the design and the [issues](../../issues) and [milestones](../../milestones)
for progress.

## Development

```sh
cargo build
cargo test
cargo run -- tests/fixtures/create-only.json
```

### Snapshot tests

`tests/snapshots.rs` runs every plan in `tests/fixtures/` through every
output format and compares the result with the saved copy in
`tests/snapshots/`. Adding a fixture adds its snapshots automatically.

When output changes on purpose, the test fails and shows a diff. To accept
the new output, either:

- review each change interactively with `cargo insta review` (install it with
  `cargo install cargo-insta`), or
- run `INSTA_UPDATE=always cargo test` and review the changed `.snap` files
  in `git diff`.

Commit the updated `.snap` files with the change that caused them.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
