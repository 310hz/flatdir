# flatdir

Browse and manage the projects that live directly under one directory (the managed root) with a built-in fuzzy picker: jump to one, archive, restore, delete, create, or clone.

## Install

Requires a Rust toolchain (`rustup`).

```sh
cargo install --path .                # from a local checkout
cargo install --git <repository-url>  # once published
```

There are no required external commands: the fuzzy picker, README preview and trash handling are built in. Optional: `zoxide` (order by frecency), `eza` (richer directory preview), `gh` and `git` (`init --git`, `clone`).

## Usage

```sh
flatdir track ~/workspace    # set the managed root (only one is supported)
flatdir                      # pick a directory and print its path
flatdir -a                   # pick from archived directories
flatdir status               # show the root, directory/archive counts and required commands
flatdir archive              # move the selected directories to the archive
flatdir restore              # move archived directories back
flatdir remove               # move the selected directories to the trash
flatdir init <name> [--git]  # create a directory (--git also creates a private GitHub repo)
flatdir clone [-s] [target]  # git clone (-s searches GitHub)
flatdir untrack              # unset the managed root
```

`flatdir` only prints a path. To `cd` into the selection, wrap it in a shell function (zsh/bash):

```sh
flatdir() {
  local out
  out="$(command flatdir "$@")" || return
  [ -z "$out" ] && return
  if { [ "$#" -eq 0 ] || [ "$1" = "-a" ]; } && [ -d "$out" ]; then
    cd "$out"
  else
    printf '%s\n' "$out"
  fi
}
```

## Configuration

`~/.config/flatdir/config.toml` (or `$XDG_CONFIG_HOME/flatdir/config.toml`):

```toml
root = "/Users/me/workspace"
# color = "light"   # optional picker color scheme (fzf --color syntax); see below
```

By default the picker uses only your terminal's own palette colors and reverse video, so it reads the same in light and dark terminals. Set `color` (for example `"light"`, `"dark"`, `"bw"`, or `"dark,prompt:4"`) to override it.

## Archive layout

Archived directories are moved to `~/.local/share/flatdir/archive/<root path relative to $HOME>/<name>`, mirroring the layout under the root. No marker files are needed.

## Safety

`remove`, `archive` and `restore` move or delete real directories, so they:

- run only on an interactive terminal (never in pipes or scripts),
- refuse to act on every candidate at once,
- cannot be made non-interactive through environment variables (the picker reads no `FZF_DEFAULT_OPTS` or `SKIM_DEFAULT_OPTIONS`),
- delete only by moving to the system trash, never with `rm`.
