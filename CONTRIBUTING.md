
# Contributing to BullG

- Use Rust 1.79+ and `edition = "2024"`.
- Run `cargo fmt` and `cargo clippy -- -D warnings` before PRs.
- Keep public APIs documented.
- Add examples and tests for new features.
- Safety first: prefer `Arc`, `DashMap`, `parking_lot` over `Mutex` where possible.
