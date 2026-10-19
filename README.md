# flatdir

A TUI for the projects that live directly under one directory: jump to one, archive, restore or trash them, and create or clone new ones, with a README or file-tree preview.

## Install

Requires a Rust toolchain (`rustup`).

```sh
cargo install --git https://github.com/310hz/flatdir
```

Then choose the directory to manage, and add the shell function that lets Enter `cd` (bash and zsh) to `.zshrc` or `.bashrc`:

```sh
flatdir root ~/workspace
eval "$(flatdir shell)"
```

No other commands are required. Some features use them when available: `zoxide` (order by frecency), `git`, `gh` (private GitHub repositories, GitHub search), `curl` (Hugging Face search) and `git-lfs`.

## Usage

```sh
flatdir            # open the TUI; press ? for the keys
flatdir root --cd  # cd into the managed root
flatdir status     # show the root, counts and optional commands
```

Archived directories are kept in `~/.local/share/flatdir/archive/`, under the same path as the root has inside `$HOME`.

## Safety

flatdir moves real directories, so it:

- runs only on an interactive terminal, never in pipes or scripts,
- refuses to act on every entry of a list at once,
- removes only by moving to the system trash, never with `rm`,
- never overwrites an existing directory when creating or cloning.
