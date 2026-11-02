# Vendored egui-baseview 0.7.2

This directory is a **patched copy** of the `egui-baseview` 0.7.2 crate
(unpacked from crates.io — see `.cargo_vcs_info.json`). It is **not** an
original part of PEJVAK RVB; it exists only so that CI builds are
reproducible and do not depend on a separate fork repository.

`[patch.crates-io]` in the workspace `Cargo.toml` redirects the
dependency to this copy:

```toml
[patch.crates-io]
egui-baseview = { path = "egui-baseview-0.7.2" }
```

## Why it is vendored

The published 0.7.2 crate does not compile with the Rust toolchain this
project is pinned to: its source uses newer syntax / edition settings
than stable Rust accepts. Upstream changes are tracked in the repo
history (commits `a840534`, `204c4a3`, `0c082cb`).

## Local modifications vs. the published crate

1. **Edition 2021** — `Cargo.toml` was changed from edition 2024 to
   2021 (`204c4a3`).
2. **let-chains rewritten** — `src/window.rs` used Rust 2024
   let-chains (`if let ... && ...`); those conditions were rewritten as
   nested `if let`s so the crate compiles under edition 2021
   (`0c082cb`). No behavioural change.

## Upgrading

Replace this directory with a newer upstream release, re-apply the two
edits above if still needed, update the `path` in the workspace
`Cargo.toml` if the directory name changes, and update this file.
