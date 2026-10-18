# コマンド仕様

出力契約: 選択結果のパスだけを stdout に出す。メッセージは全て stderr。シェル関数がこの stdout を受けて `cd` する(引数なしと `-a` のときだけ)。

| コマンド | 挙動・副作用 |
|---|---|
| (なし) | root 直下を zoxide スコア降順で fzf 選択し、パスを出力。読み取りのみ |
| `-a` | アーカイブ済みを fzf 選択し、パスを出力。読み取りのみ |
| `track <path>` | 管理 root を設定(1 つだけ。HOME 配下のみ)。設定済みなら先に `untrack` |
| `untrack` | 管理 root の設定を解除。設定ファイルのみ変更 |
| `status` | 管理 root・直下のディレクトリ数・アーカイブ先と件数・設定パス・`fzf_opts`・外部コマンドの有無を表示(stdout)。読み取りのみで、root 未設定や読めない場合も報告して正常終了 |
| `remove` | 複数選択したディレクトリを `trash` コマンドへ送る |
| `archive` | 複数選択したものをアーカイブへ **移動**。その後 `.venv`・`node_modules` 等の削除を 1 件ずつ確認(はいなら trash) |
| `restore` | アーカイブ済みを複数選択し、root 直下へ **移動**。旧マーカーがあれば削除 |
| `init <name> [--git]` | root 直下に作成。`--git` は GitHub に private リポジトリを作成して clone、README の初回 push まで行う |
| `clone [-s] [target]` | root 直下に `git clone`。`target` は URL・`owner/repo`・`repo`(自分のアカウント)。見つからなければ検索。同名があれば確認のうえ trash して上書き |

`remove` / `archive` / `restore` は対話端末が必要で、全件選択を拒否する(→ [safety.md](../concepts/safety.md))。

## パスと設定

- 設定: `$XDG_CONFIG_HOME/flatdir/config.toml`(既定 `~/.config/flatdir/`)。`root`(文字列)、`fzf_opts`(文字列の配列。`--filter`・`-1`・`-0` 等の危険なものは読み込みで拒否)。
- アーカイブ: `$XDG_DATA_HOME/flatdir/archive/<root の HOME 相対パス>/<name>`(既定 `~/.local/share/flatdir/archive/`)。root 直下と同じ階層名を保つので、目印ファイルは不要。
- 一覧から除外: 隠しディレクトリ、シンボリックリンク、名前に TAB/改行を含むもの。

## 外部コマンド

必須: `fzf`、`trash`(`remove`・`archive` の削除確認・`clone` 上書き)。任意: `zoxide`(無ければ並び順が名前順)、`eza`・`rich`(プレビュー)。`gh`・`git`: `init --git` と `clone`。

プレビューは自分自身の隠しサブコマンド(`__preview`・`__clone-preview`)を fzf から呼ぶ。
