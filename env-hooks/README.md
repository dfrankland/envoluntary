# env-hooks

Shell integration library for building direnv-like utilities. Provides direnv
core logic for shell-agnostic management of the state of environment variable
exporting and unsetting across different shell types (bash, zsh, fish, Nushell).

## Features

- **Multi-shell support**: Works with bash, zsh, fish, and Nushell shells
- **Environment state management**: Manages the export and unset state of
  environment variables, essential for direnv-like functionality
- **JSON output**: Can export environment variables in JSON format for
  programmatic access
- **Environment hooks**: Integration hooks for seamless environment loading

## Example

For a practical example of using `env-hooks`, see the [direnv example](https://github.com/dfrankland/envoluntary/tree/main/env-hooks/examples/direnv).
It's a simplified implementation of `direnv` that demonstrates the core
functionality of this library:

- Hooks into bash, zsh, fish, and Nushell shells
- Reads `.envrc` files by walking up the directory hierarchy
- Exports environment variables from those files

To run the example:

```bash
cargo run --example direnv -- --help
cargo run --example direnv -- hook bash
cargo run --example direnv -- export bash
```

This is a great starting point for understanding how to integrate `env-hooks`
into your own shell-based utilities.

## Part of envoluntary

This library is a core component of [envoluntary](https://github.com/dfrankland/envoluntary),
an automatic Nix development environment management tool for your shell.

For more information about the broader project, see the [main README](https://github.com/dfrankland/envoluntary/blob/main/README.md).
