# flatdir

1 つのディレクトリ(管理 root)直下のプロジェクトを fzf で選んで、移動・アーカイブ・削除・作成・clone するCLI。

## インストール

Rust の環境が必要(`rustup`)。

```sh
cargo install --path .                 # ローカルのソースから
cargo install --git <リポジトリURL>     # 公開後はこちら
```

依存コマンド: `fzf`、`trash`(必須)。`zoxide`(使用頻度順)、`eza`・`rich`(プレビュー)、`gh`・`git`(`init --git`・`clone`)は任意または機能限定。

## 使い方

```sh
flatdir track ~/workspace   # 管理 root を設定(1 つだけ)
flatdir                     # fzf で選んでパスを出力
flatdir -a                  # アーカイブ済みから選ぶ
flatdir archive             # 複数選択してアーカイブへ移動
flatdir restore             # アーカイブから戻す
flatdir remove              # 複数選択して trash へ
flatdir init <name> [--git] # 新規作成(--git は GitHub に private リポジトリも作る)
flatdir clone [-s] [target] # git clone(-s は GitHub を検索)
flatdir untrack | list
```

`cd` するにはシェル関数で包む(zsh/bash)。

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

## 設定

`~/.config/flatdir/config.toml`

```toml
root = "/Users/me/workspace"
fzf_opts = []   # 例: ["--color=light"]
```

色は端末テーマと fzf の既定に任せている(ライト/ダーク共通)。

## 安全のために

`remove`・`archive`・`restore` は実ディレクトリを動かす。対話端末でしか動かず、全件選択は拒否する。詳細は `agents/docs/concepts/safety.md`。

アーカイブは `~/.local/share/flatdir/archive/<rootのHOME相対パス>/` 直下にそのまま置かれる。
