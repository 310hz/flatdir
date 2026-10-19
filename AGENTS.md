# AGENTS.md

管理 root(1 つ)直下のディレクトリを TUI で選んで移動・アーカイブ・削除する CLI `flatdir`。dotfiles にあった sh 実装(`src/flatdir`)の Rust 再実装。ユーザーは Rust に不慣れなので、構成や手順は丁寧に説明する。

## 最重要: 実ディレクトリを移動・削除するツール

`remove` / `archive` / `restore` / `clone` / `init` / `root` は実データを変更する。旧 sh 版は、テスト中に非対話の fzf(`--filter=`)で全件が選択され、全プロジェクトの移動・削除を起こした。

- **実環境(実 HOME・実 trash)に対してバイナリを実行しない。** 検証は `cargo test` に任せる。手動で試すなら、`env -i` で環境を作り直し、使い捨ての `HOME` だけで動かす(`tests/cli.rs` が手本。TUI は pty で操作する)。ゴミ箱への Remove はサンドボックス外のゴミ箱に届くので、実バイナリでは実行しない(`tests/tui.rs` が偽の trasher で検証する)。
- 安全ガードは [concepts/safety.md](agents/docs/concepts/safety.md)。弱める変更(TTY 判定の撤廃、全件選択の許可、環境変数で挙動を変えられる実装への変更、削除操作の追加・対象の拡大)は、ユーザーに確認してから行う。
- 新しい破壊的コマンドを足すときは、ガードを通すこと、実環境に触れないテストを同時に書くこと。
- `cargo install` / `just install` など、グローバル環境を変える操作はユーザーが明示的に頼んだときだけ。依存のキャッシュ以外でグローバルに書かない。

## 開発

- タスクランナーは `just`(`just check` で fmt・clippy・test)。
- 構成: ロジックは `src/lib.rs` 配下、`src/main.rs` は引数解釈と実環境の組み立てだけ。破壊的操作は `ops.rs`、副作用(trash・確認・zoxide・`clone -s` のピッカー)は `services.rs` / `select.rs` の trait の裏にあり、テストでは偽物を注入する。
- 色は端末のパレット(ANSI 0〜15)と反転・太字・薄字だけで組む。ユーザーの端末はライトモードで、ダークモードとも共通で読めることが条件。256 色や RGB を足さない(`picker_uses_only_palette_colors` が検査する)。`color` 設定は `clone -s` のピッカー用のみ。
- 起動速度は最優先(最初のフレームまで数 ms)。起動経路にサブプロセスや重い読み込みを足さない([concepts/tui.md](agents/docs/concepts/tui.md))。
- 外部コマンドに依存しない方針。必須コマンドは無く、`zoxide`・`eza`・`gh`・`git` は任意機能のためだけに使う。
- 公開は `cargo install --git <URL>` 程度で、crates.io には出さない。

## ドキュメント

[agents/docs/status.md](agents/docs/status.md) → [agents/docs/index.md](agents/docs/index.md) の順に読む。`README.md` は利用者向け。
