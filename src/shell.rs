/// A program cannot change its parent shell's directory, so `flatdir` only prints the chosen path.
/// This function runs it with its output captured and does the `cd`, only for the TUI and
/// `root --cd`: other subcommands also print paths that must stay output. Works in bash and zsh.
/// Printed by `flatdir shell`; the README shows the same text (a test keeps them identical).
pub const SHELL_FUNCTION: &str = r#"flatdir() {
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
"#;

/// Shown when the path went straight to a terminal, which means no shell function captured it.
pub const MISSING_FUNCTION_HINT: &str = "flatdir only prints the path. To cd into it, add this to your shell config: eval \"$(flatdir shell)\"";
