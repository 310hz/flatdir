# flatdir

A TUI for the projects that live directly under one directory (the managed root): jump to one, archive, restore, or delete them, with a README or file-tree preview.

## Install

Requires a Rust toolchain (`rustup`).

```sh
cargo install --path .                # from a local checkout
cargo install --git <repository-url>  # once published
```

There are no required external commands. Optional: `zoxide` (order by frecency), `gh` and `git` (`init --git`, `clone`).

## Usage

```sh
flatdir track ~/workspace    # set the managed root (only one is supported)
flatdir                      # open the TUI
flatdir status               # show the root, directory/archive counts and optional commands
flatdir init <name> [--git]  # create a directory (--git also creates a private GitHub repo)
flatdir clone [-s] [target]  # git clone (-s searches GitHub)
flatdir untrack              # unset the managed root
```

### Keys in the TUI

| Key | Action |
|---|---|
| type | fuzzy filter |
| `Up` / `Down`, `PageUp` / `PageDown` | move |
| `Enter` | print the path of the current directory (the shell function below `cd`s to it) |
| `Tab` / `Shift+Tab` | mark the current entry and move down / up |
| `Ctrl+A` | switch between the Tracked and Archived lists (marks are reset) |
| `Ctrl+T` | toggle the archived state of the marked entries (or the current one): archive in the Tracked list, restore in the Archived list |
| `Ctrl+R` | move the marked entries (or the current one) to the Trash |
| `Ctrl+O` | open the current directory in the file manager (Finder on macOS, `xdg-open` elsewhere) |
| `Ctrl+P` | switch the preview: README (Markdown) <-> file tree. Without a README the tree is shown |
| `Ctrl+U` | clear the filter |
| mouse wheel | over the list: move the cursor; over the preview: scroll it |
| left click | on a list row: move the cursor there |
| right click | on a list row: mark / unmark it (like `Tab`, without moving on) |
| `Esc` / `Ctrl+C` | quit |

While the TUI is open the terminal reports the mouse to flatdir, so selecting text with the mouse needs `Shift` (or `Option` in some terminals) held down.

The shortcuts are always shown at the top, and the `Ctrl+T` label says what it will do in the current list.

`flatdir` only prints a path. To `cd` into the selection, wrap it in a shell function (zsh/bash):

```sh
flatdir() {
  local out
  out="$(command flatdir "$@")" || return
  [ -z "$out" ] && return
  if [ "$#" -eq 0 ] && [ -d "$out" ]; then
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
sound = false   # macOS: play the Finder "move to trash" sound after removing
# color = "light"   # color scheme of the `clone -s` picker (fzf --color syntax)
```

The TUI uses only your terminal's own palette colors plus bold/reverse/dim, so it reads the same in light and dark terminals.

## Archive layout

Archived directories are moved to `~/.local/share/flatdir/archive/<root path relative to $HOME>/<name>`, mirroring the layout under the root. No marker files are needed.

## Safety

Archiving, restoring and removing move real directories, so the TUI:

- runs only on an interactive terminal (never in pipes or scripts),
- refuses to act on every entry of a list at once,
- treats pasted text as text, so a paste can never trigger a shortcut,
- is not influenced by `FZF_DEFAULT_OPTS` or any other environment variable,
- removes only by moving to the system trash (`Ctrl+R`), never with `rm`, and never deletes anything inside an archived project (`node_modules` and friends are only reported).
