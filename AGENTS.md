# AGENTS.md

管理 root(1 つ)直下のディレクトリを fzf で選んで移動・整理する CLI `flatdir`。dotfiles にあった sh 実装(`src/flatdir`)の Rust 再実装。ユーザーは Rust に不慣れなので、構成や手順は丁寧に説明する。

## 最重要: 実ディレクトリを移動・削除するツール

`remove` / `archive` / `restore` / `clone` / `init` / `track` / `untrack` は実データを変更する。旧 sh 版は、テスト中に非対話の fzf(`--filter=`)で全件が選択され、全プロジェクトの移動・削除を起こした。

- **実環境(実 HOME・実 fzf・実 trash)に対してバイナリを実行しない。** 検証は `cargo test` に任せる。手動で試すなら、`env -i` で環境を作り直し、使い捨ての `HOME`、偽の `fzf` と `trash` だけを `PATH` に置く(`tests/cli.rs` が手本)。
- 安全ガードは [concepts/safety.md](agents/docs/concepts/safety.md)。弱める変更(TTY 判定の撤廃、全件選択の許可、fzf 環境変数の無条件引き継ぎ)は、ユーザーに確認してから行う。
- 新しい破壊的コマンドを足すときは、ガードを通すこと、実環境に触れないテストを同時に書くこと。
- `cargo install` / `just install` など、グローバル環境を変える操作はユーザーが明示的に頼んだときだけ。依存のキャッシュ以外でグローバルに書かない。

## 開発

- タスクランナーは `just`(`just check` で fmt・clippy・test)。
- 構成: ロジックは `src/lib.rs` 配下、`src/main.rs` は引数解釈と実環境の組み立てだけ。副作用(fzf・trash・確認・zoxide)は `services.rs` / `select.rs` の trait の裏にあり、テストでは偽物を注入する。
- 色は自前でハードコードしない。ユーザーの端末はライトモードで、ダークモードとも共通で読めることが条件。配色は端末テーマと fzf に任せ、必要なら `config.toml` の `fzf_opts` で上書きする。
- 公開は `cargo install --git <URL>` 程度で、crates.io には出さない。

## ドキュメント

[agents/docs/status.md](agents/docs/status.md) → [agents/docs/index.md](agents/docs/index.md) の順に読む。`README.md` は利用者向け。
