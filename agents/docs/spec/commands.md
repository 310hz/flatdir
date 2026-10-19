# コマンド仕様

出力契約: 選択結果のパスだけを stdout に出す。メッセージは全て stderr。TUI は stderr に描画する(stdout はシェル関数が `$(...)` で受け取る)。シェル関数は引数なしのときだけ `cd` する。

| コマンド | 挙動・副作用 |
|---|---|
| (なし) | TUI を開く。Enter で現在行のパスを出力して終了。対話端末が必要 |
| `root [<path>] [--unset \| --cd]` | 引数なし: 管理 root を stdout に出す(未設定ならエラー)。`--cd`: 同じく出力し、シェル関数がそこへ `cd` する(関数が無く stdout が端末なら案内を stderr に出す)。`<path>`: 管理 root を設定(HOME 配下のみ)。設定済みでも入れ替わる。`--unset`: 解除。設定ファイルのみ変更。旧 root のアーカイブは各パスの下に残り、その root に戻すとまた見える |
| `status` | 管理 root・直下のディレクトリ数・アーカイブ先と件数・設定パス・`color`・任意コマンドの有無を表示(stdout)。読み取りのみで、root 未設定や読めない場合も報告して正常終了 |
| `init <name> [--git]` | root 直下に作成。`--git` は GitHub に private リポジトリを作成して clone、README の初回 push まで行う |
| `clone [-s] [target]` | root 直下に `git clone`。`target` は URL・`owner/repo`・`repo`(自分のアカウント)。見つからなければ検索(`-s` も同じピッカー)。同名があれば確認のうえゴミ箱へ送って上書き |

旧版の `-a`・`archive`・`restore`・`remove`・`list` は廃止した(TUI に統合、または管理 root が 1 つのため不要)。

## TUI

[concepts/tui.md](../concepts/tui.md) を参照。

## パスと設定

- 設定: `$XDG_CONFIG_HOME/flatdir/config.toml`(既定 `~/.config/flatdir/`)。`root`(文字列)、`sound`(真偽、既定 false。macOS でゴミ箱の効果音)、`color`(`clone -s` のピッカーの配色。fzf の `--color` と同じ書式)。旧版の `fzf_opts` など未知のキーは無視する。
- アーカイブ: `$XDG_DATA_HOME/flatdir/archive/<root の HOME 相対パス>/<name>`(既定 `~/.local/share/flatdir/archive/`)。root 直下と同じ階層名を保つので、目印ファイルは不要。旧版の `.flatdir_archived` が残っていても一覧に出て、復元時に取り除く。
- 一覧から除外: 隠しディレクトリ、シンボリックリンク、名前に TAB/改行を含むもの。

## 外部コマンド

必須は無い(ゴミ箱は `trash` crate、README は `pulldown-cmark` で自前描画、ツリーは `ignore` crate)。任意: `zoxide`(無ければ並び順が名前順)、`gh`・`git`(`init --git` と `clone` のみ。ssh 鍵や GitHub 認証がユーザー環境に依存するため外部のまま)。
