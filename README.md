# flatdir

A TUI for the projects that live directly under one directory (the managed root): jump to one, archive, restore, or delete them, with a README or file-tree preview.

## Install

Requires a Rust toolchain (`rustup`).

```sh
cargo install --path .                # from a local checkout
cargo install --git <repository-url>  # once published
```

There are no required external commands. Optional: `zoxide` (order by frecency), `git` (cloning), `gh` (new private GitHub repositories, GitHub search), `curl` (Hugging Face search) and `git-lfs` (large files of Hugging Face clones).

## Usage

```sh
flatdir root ~/workspace     # set the managed root (replaces the current one)
flatdir                      # open the TUI
flatdir status               # show the root, directory/archive counts and optional commands
flatdir root                 # print the managed root
flatdir root --cd            # cd into it (needs the shell function below)
flatdir root --unset         # unset it
```

### Keys in the TUI

| Key | Action |
|---|---|
| type | fuzzy filter (`?` is taken by the help, so it cannot be typed) |
| `Up` / `Down`, `PageUp` / `PageDown` | move |
| `Enter` | print the path of the current directory (the shell function below `cd`s to it) |
| `Tab` / `Shift+Tab` | mark the current entry and move down / up |
| `Ctrl+Q` | clear all marks |
| `Ctrl+T` | switch between the Active and Archived lists (marks are reset) |
| `Ctrl+A` | toggle the archived state of the marked entries (or the current one): archive in the Active list, restore in the Archived list |
| `Ctrl+R` | move the marked entries (or the current one) to the Trash |
| `Ctrl+O` | open the current directory in the file manager (Finder on macOS, `xdg-open` elsewhere) |
| `Ctrl+P` | switch the preview: README (Markdown) <-> file tree. Without a README the tree is shown |
| `Ctrl+N` | open the New view (below) |
| `Ctrl+U` | clear the filter |
| mouse wheel | over the list: move the cursor; over the preview: scroll it |
| left click | on a list row: move the cursor there |
| right click | on a list row: mark / unmark it (like `Tab`, but the cursor does not move) |
| `?` | show / hide the list of shortcuts (`Esc` also closes it) |
| `Esc` / `Ctrl+C` | quit |

While the TUI is open the terminal reports the mouse to flatdir, so selecting text with the mouse needs `Shift` (or `Option` in some terminals) held down.

The title row only points to the help (`? Help`); the help window lists the shortcuts, and its `Ctrl+A` label says what it will do in the current list.

### New directories and clones

`Ctrl+N` swaps the list for the New view. The query line becomes the name (what you had typed is kept), and `Tab` / `Shift+Tab` chooses the kind:

| Kind | Enter with the typed text |
|---|---|
| Directory | creates `<root>/<name>` |
| Git (private) | creates a private GitHub repository with `gh`, clones it to `<root>/<name>` and pushes a first commit |
| Clone: GitHub | `owner/repo` or a URL is cloned; any other text is searched with `gh search repos` |
| Clone: HF Model / Dataset / Space | the same for Hugging Face (search uses the public Hub API through `curl`) |

The line above the results says exactly what Enter will do. After a search, choose a result with `Up` / `Down` (or `j` / `k`) and press Enter to clone it. While results are shown the name is locked; `Esc` clears them so you can edit the search, and a second `Esc` goes back to the list. For Hugging Face, `Ctrl+L` decides whether Git LFS files are downloaded (off by default: they stay as small pointer files, since model weights can be huge). Nothing is ever overwritten: a name that exists in the root or in the archive is refused. When it is done, the list comes back with the cursor on the new directory, so Enter goes there. `Ctrl+N` (or `Esc` when no results are shown) goes back to the list without creating anything.

Git runs without a terminal while the TUI is open, so it cannot ask for passwords or passphrases; use an SSH agent or a credential helper.

`flatdir` only prints the path (a program cannot change its parent shell's directory). To `cd` into the selection, load a small shell function (bash and zsh):

```sh
eval "$(flatdir shell)"   # put this in .zshrc / .bashrc (`flatdir shell zsh` works too)
```

It is this function; you can also paste it into your config instead of using `eval`:

```sh
flatdir() {
  local out
  out="$(command flatdir "$@")" || return
  [ -z "$out" ] && return
  case "$*" in
    "" | "root --cd")
      if [ -d "$out" ]; then
        cd "$out"
        return
      fi
      ;;
  esac
  printf '%s\n' "$out"
}
```

## Configuration

`~/.config/flatdir/config.toml` (or `$XDG_CONFIG_HOME/flatdir/config.toml`):

```toml
root = "/Users/me/workspace"
sound = false   # macOS: play the Finder "move to trash" sound after removing
```

The TUI uses only your terminal's own palette colors plus bold/reverse/dim, so it reads the same in light and dark terminals.

## Archive layout

Archived directories are moved to `~/.local/share/flatdir/archive/<root path relative to $HOME>/<name>`, mirroring the layout under the root. No marker files are needed.

## Safety

Archiving, restoring and removing move real directories, so the TUI:

- runs only on an interactive terminal (never in pipes or scripts),
- ignores the list's actions (trash, archive, open) while the New view hides the list, and never overwrites an existing directory when creating or cloning,
- refuses to act on every entry of a list at once,
- treats pasted text as text, so a paste can never trigger a shortcut,
- is not influenced by `FZF_DEFAULT_OPTS` or any other environment variable,
- removes only by moving to the system trash (`Ctrl+R`), never with `rm`, and never deletes anything inside an archived project (`node_modules` and friends are only reported).
