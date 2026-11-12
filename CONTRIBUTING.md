# Contributing to ferrocord

Thanks for your interest! Bug reports, signatures, plugins, docs and code are all welcome.

## Getting started

```bash
git clone https://github.com/Official-OptiNex/ferrocord
cd ferrocord
cargo test --workspace
```

To build the example plugin you also need the WebAssembly target:

```bash
rustup target add wasm32-unknown-unknown
```

## Before opening a pull request

- `cargo fmt --all` leaves no changes
- `cargo clippy --workspace --all-targets` has no warnings
- `cargo test --workspace` passes
- New behaviour has tests where it's practical
- Unsafe code has a `// SAFETY:` comment explaining why it's sound

## Contributing signatures

When a client update breaks a signature:

1. Include the client version and platform in your PR.
2. Keep patterns as short as possible while still matching exactly once.
3. Use wildcards (`??`) for addresses and offsets that change between builds.
4. Run `ferrocord doctor`, launch the client, and include the relevant log lines.

## Commit messages

Use the imperative mood and keep the first line under about 72 characters, e.g. `Add PE section lookup for Windows`.

## Ground rules

- Be respectful. Harassment of any kind isn't tolerated.
- Don't submit code that steals tokens or credentials, spams, self-bots, or harms other users. It will be
  rejected and reported.

## License

By contributing, you agree that your contributions will be dual licensed under MIT OR Apache-2.0, as
described in the [README](README.md#-license).
