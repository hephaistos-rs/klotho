# Klotho
**Self-hosted git, where code is spun**

Klotho (also spelled Clotho) is the first of the Moirai, the three Fates of Greek mythology. Klotho spins the thread of life, her sister Lachesis measures it, and Atropos cuts it when the life is complete.

In the Hephaistos project, Klotho is where your code's life begins. She is a self-hosted git platform: every repository starts here, and every commit adds to the thread.

> ⚠️ **Status:** Klotho is in early development and not yet ready for production use.

## The Moirai

Klotho is one of three components that are designed to work together, each named after one of the sisters:

| Component | Sister | Role in the myth | Role in Hephaistos |
|-----------|--------|------------------|--------------------|
| **Klotho** | The spinner | Spins the thread of life | Git platform: where code is born |
| **Lachesis** | The measurer | Measures each thread's length | Runner: the stages code grows through |
| **Atropos** | The one who cannot be turned | Cuts the thread when a life is complete | Hosting: where the finished work is released |

Each component can run on its own, but together they take your code from its first commit to a running application, entirely on your own infrastructure.

## Features

<!-- TODO: fill in as features land -->

- Git repository hosting
- Built for self-hosting
- Written in Rust
- Integrates with Lachesis for CI/CD and Atropos for deployment

## Getting started

<!-- TODO: add installation instructions -->

### Requirements

- TODO

### Installation

```sh
# TODO
```

### Configuration

<!-- TODO: document configuration options -->

## Contributing

Contributions are welcome! Please open an issue to discuss larger changes before submitting a pull request.

Before sending a change, run every check it has to pass:

```sh
cargo xtask ci
```

That's format, clippy with `-D warnings`, the tests (which drive the real `git` client), `cargo deny check` and the single-binary release build. It needs `git`, `cargo install cargo-deny --locked` and `cargo install topcoat-cli --version 0.9.0 --locked`. There's no hosted CI; Lachesis will run this same command.

<!-- TODO: link to CONTRIBUTING.md if you add one -->

## License

<!-- TODO: choose a license, e.g. MIT, Apache-2.0, or AGPL-3.0 -->

---

Part of [Hephaistos](https://github.com/hephaistos-rs), forged in Rust.