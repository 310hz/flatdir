# コマンド仕様

出力の約束: stdout に出すのはパスだけ(シェル関数が `$(...)` で受け取る)。メッセージはすべて stderr。TUI も stderr に描画する。

| コマンド | 挙動・副作用 |
|---|---|
| (なし) | TUI を開く([spec/tui.md](tui.md))。対話端末が必要 |
| `root` | 管理 root を stdout に出す(未設定ならエラー) |
| `root --cd` | 同じく出力し、シェル関数がそこへ `cd` する。関数が無く stdout が端末なら、案内を stderr に出す |
| `root <path>` | 管理 root を設定する(HOME 配下の既存ディレクトリのみ)。設定済みでも入れ替わる。変わるのは設定ファイルだけ |
| `root --unset` | 設定を解除する。旧 root のアーカイブは残り、その root に戻すとまた見える |
| `status` | 管理 root、直下のディレクトリ数、アーカイブ先と件数、設定ファイルのパス、任意コマンドの有無を stdout に出す。読み取りのみで、root が未設定・読めない場合も報告して正常終了 |
| `shell [zsh\|bash]` | シェル関数を出力する(どちらも同じ文面)。`--help` の一覧には出さない |

シェル関数が `cd` するのは、引数なし(TUI)と `root --cd` のときだけ。他の出力はそのまま表示する。

## パスと設定

- 設定: `$XDG_CONFIG_HOME/flatdir/config.toml`(既定 `~/.config/flatdir/`)。キーは `root`(文字列)と `sound`(真偽、既定 false。macOS で `Ctrl+R` のときにゴミ箱の効果音)。未知のキーは無視する。
- アーカイブ: `$XDG_DATA_HOME/flatdir/archive/<root の HOME 相対パス>/<name>`(既定 `~/.local/share/flatdir/archive/`)。この直下全体がアーカイブで、目印ファイルは使わない。旧版の目印 `.flatdir_archived` は復元時に取り除く。
- 一覧に出さないもの: 隠しディレクトリ、シンボリックリンク、ファイル、名前に TAB / 改行を含むもの。

## 外部コマンド

必須のものは無い(ゴミ箱は `trash` crate、README 表示は `pulldown-cmark`、ツリーは `ignore` crate)。次は任意機能のためだけに使う。

| コマンド | 使う機能 | 無いとき |
|---|---|---|
| `zoxide` | 一覧の並び(frecency 順) | 名前順 |
| `git` | New ビューの clone と Git (private) | その操作がエラー |
| `gh` | Git (private)、GitHub の検索 | 同上 |
| `curl` | Hugging Face の検索 | 同上 |
| `git-lfs` | Hugging Face の clone で LFS ファイルを落とすとき | ポインタのまま |
| `open` / `xdg-open` | `Ctrl+O` | エラー |

git・gh は、ssh 鍵や GitHub の認証がユーザー環境に依存するので外部コマンドのまま使う。
