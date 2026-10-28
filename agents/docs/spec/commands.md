# コマンド仕様

出力の約束: stdout に出すのはパスだけ(シェル関数が `$(...)` で受け取る)。メッセージはすべて stderr。TUI も stderr に描画する。

| コマンド | 挙動・副作用 |
|---|---|
| (なし) | TUI を開く([spec/tui.md](tui.md))。対話端末が必要 |
| `root` | 管理 root を stdout に出す(未設定ならエラー) |
| `root --cd` | 同じく出力し、シェル関数がそこへ `cd` する。関数が無く stdout が端末なら、案内を stderr に出す |
| `root <path>` | 管理 root を設定する(HOME 配下の既存ディレクトリのみ)。設定済みでも入れ替わる。変わるのは設定ファイルだけ |
| `root --unset` | 設定を解除する。旧 root のアーカイブの記録は残り、その root に戻すとまた効く |
| `status` | 管理 root、直下のディレクトリ数(アーカイブ済みを含む)とアーカイブ件数、設定・状態ファイルのパス、任意コマンドの有無を stdout に出す。読み取りのみで、root が未設定・読めない場合も報告して正常終了 |
| `shell [zsh\|bash]` | シェル関数を出力する(どちらも同じ文面)。`--help` の一覧には出さない |

シェル関数が `cd` するのは、引数なし(TUI)と `root --cd` のときだけ。他の出力はそのまま表示する。

## パスと設定

- 設定: `$XDG_CONFIG_HOME/flatdir/config.toml`(既定 `~/.config/flatdir/`)。キーは `root`(文字列)だけ。未知のキー(廃止した `sound` など)は無視する。
- アーカイブ: ディレクトリは root 直下に置いたまま、一覧(Active)に出さないだけ。`ls` や Finder には普通に見える。どれがアーカイブ済みかは状態ファイル `$XDG_STATE_HOME/flatdir/state.json`(既定 `~/.local/state/`)に記録する。flatdir だけが書くファイルで、毎回全体を書き直す。
  - 形式: `{"archived": {"<絶対パス>": {"archived_at": <UNIX 秒>}}}`。キーが絶対パスなので、root を切り替えても別 root の同名ディレクトリは隠れない。項目を足すときは各エントリのオブジェクトに足す。
  - TUI の起動時と操作後に、今の root 直下に無くなった記録(手で消した・リネームした)を捨てる。リネームは削除と同じ扱いで、新しい名前は Active に出る。root を読めないときは何もしない。`status` は読むだけで捨てない。
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
