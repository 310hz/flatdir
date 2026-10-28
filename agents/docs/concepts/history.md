# 経緯: shell 版から Rust 版の独立リポジトリへ

この文書だけで、なぜ今の形なのか、旧版とどこが違うのかが分かるように書いてある。`agents/refs/` はコミットされないローカル専用なので、ここから先の説明は refs が無い環境でも成り立つ。

## 1. もとは dotfiles の一部だった

- bash 実装が、ユーザーの dotfiles リポジトリ(`git@github.com:310hz/dotfiles.git`)の `src/flatdir/` と `modules/flatdir.sh`(`cd` するシェル関数)にあった。初回コミットは 2025-12-13(`05ce75b`)。
- 旧版の最終状態はコミット `87bea1a`。独立後に dotfiles から削除したが、履歴に残っている:

  ```sh
  git -C <dotfiles> show 87bea1a:src/flatdir/entrypoint.sh      # 入口
  git -C <dotfiles> show 87bea1a:src/flatdir/lib/archive.sh     # 各サブコマンドは lib/*.sh
  git -C <dotfiles> show 2690af1:modules/flatdir.sh             # 旧 cd ラッパー関数
  git -C <dotfiles> log --follow -- src/flatdir/entrypoint.sh   # 全履歴
  ```

- ユーザーの手元には同じ内容を `agents/refs/flatdir-sh/` に写してある(ローカルのみ)。
- 旧版のコマンド: 引数なし(fzf 選択)、`-a`、`track` / `untrack` / `list`、`remove` / `archive` / `restore`、`init`、`clone`。管理 root は複数持てた。選択は外部コマンドの `fzf`、削除は `trash`、プレビューは `rich` / `eza`。アーカイブ済みの目印として、各ディレクトリに `.flatdir_archived` を置いていた。

## 2. なぜ Rust で独立させたか

1. **2026-10-14**: 「flatdir が遅い」という依頼。原因は言語ではなくアルゴリズムだった(パスごとに `zoxide query -ls` を実行していた、アーカイブを `find` で全走査していた、など)。sh のまま直した(dotfiles `7dcbbf3`、0.35 秒 → 0.03 秒)。
2. **2026-10-18 の事故**: その計測のために `FZF_DEFAULT_OPTS="--filter=" flatdir restore` と `remove` を実環境で実行した。`--filter=` の fzf は全件を出力するので、全アーカイブが解除され、全プロジェクトが `trash` に移動された(ユーザーの `rm` エイリアスが `trash` だったので復旧できた)。原因は、fzf の出力をそのまま移動・削除へ流し、確認も実行条件も無かった設計。詳しい対策は [safety.md](safety.md)。
3. ユーザーの方針: 「事故を仕組みで防ぎたい」「ツール一式を 1 つのパッケージにしたい(`cargo install --git` で入る)」「誰が入れても依存ごとそのまま動く」「起動は限りなく速く」。そこで Rust で書き直し、dotfiles から切り離した。

## 3. 旧版から引き継いだもの・変えたもの

| 項目 | 旧(shell) | 新(Rust) |
|---|---|---|
| 管理 root | 複数 | **1 つだけ**(`flatdir root`) |
| 選択・移動・整理 | fzf と、`-a` `remove` `archive` `restore` の別コマンド | **1 つの TUI** に統合([spec/tui.md](../spec/tui.md)) |
| 作成・clone | `init [--git]`、`clone [-s]`(同名は確認して上書き) | TUI の **New ビュー**に統合。Hugging Face の clone と検索を追加。**上書きはしない** |
| アーカイブ | `~/.local/share/flatdir/archive/<HOME 相対パス>/<name>` へ移動し、各ディレクトリに目印 `.flatdir_archived` | **移動しない**。root に置いたまま状態ファイルに記録して一覧から隠す([spec/commands.md](../spec/commands.md))。目印は使わない |
| 設定 | `src/flatdir/config`(`DIRS="a:b"`、リポジトリ内) | `~/.config/flatdir/config.toml`(`root`、`sound`)。**旧設定は読まない** |
| 外部コマンド | fzf、trash、rich、eza | 必須は無し(`trash` crate、自前 TUI、自前 Markdown 描画)。`zoxide`・`git`・`gh`・`curl`・`git-lfs` は任意機能 |
| 並び順 | zoxide のスコア降順 | 同じ(起動を待たせないよう非同期で適用) |
| 表示名 | root 1 つなら root 相対 | 同じ |
| アーカイブ時の掃除 | `node_modules` 等を 1 件ずつ確認して削除 | **廃止**。残っているものを報告するだけで、自動削除はしない |
| `cd` | `modules/flatdir.sh` の関数 | `flatdir shell` が出す関数([tui.md](tui.md) の「Enter で cd する仕組み」)。`root --cd` でも `cd` する |

## 4. 判断の記録(却下した案を含む)

- **sh のまま安全策を足す案**(TTY 必須、全件拒否、確認): 提案したが、ユーザーが「1 つのパッケージにしたい」ため Rust になった。安全策は Rust 版に取り込んだ。
- **毎回の確認プロンプト**: ユーザー体験が下がるとして却下。代わりに、対話端末必須・全件選択の拒否・環境変数に影響されない設計で守る。
- **Python / Go**: 候補に挙げたが、Rust を選んだ(CLI/TUI ライブラリが成熟、単一バイナリ、`cargo install --git` で配布できる)。
- **skim で TUI を作る案**: 行ごとの色、複数アクション、ヘッダの固定レイアウトが足りず、ratatui で自前の TUI にした。`clone -s` のピッカーにだけ残していた skim も、clone を TUI に統合したときに外した。
- **アーカイブと削除をモードで切り替える案**: ユーザーの意図と違い撤回。アーカイブ/復元(`Ctrl+A`)と削除(`Ctrl+R`)は、それぞれ 1 キーの操作。
- **アーカイブで別の場所へ移動する方式**(旧版と Rust 版の初期): パスが変わるせいで、エディタの履歴・zoxide・venv 内の絶対パス・開いているシェルが壊れる不具合が多く、ユーザーの判断で「root に置いたまま隠す」方式に変えた(2026-10-28)。記録の形式は、ツールだけが書く・項目を増やせる・依存が既にある、の理由で JSON。`config.toml` に混ぜる案、TOML・JSONL・プレーンテキストは採らなかった。各ディレクトリに目印ファイルを置く方式は、git の未追跡ファイルになるので採らない。リネームの追跡(中身のハッシュなど)はしない(ユーザーの判断)。
- **色**: ユーザーの端末はライトモード。ダークモードとも共通で読めるよう、端末のパレット(ANSI 0〜15)と反転・太字だけを使う。

## 5. 旧版と挙動を比べたいとき

- 旧ソースは **読むだけ**にする。旧版の `remove` / `restore` / `archive` は、非対話の fzf と組み合わさると全件に作用する(事故の原因)。実環境で実行しない。
- 比較の目安は、[spec/commands.md](../spec/commands.md)・[spec/tui.md](../spec/tui.md) と、旧版の `lib/*.sh`。旧版の README(`agents/refs/flatdir-sh/README.md`、または履歴の `src/flatdir/README.md`)も参考になる。

## 6. 独立後の運用メモ

- ディレクトリを移動したら `cargo clean` する。`target/` にビルド時の絶対パスが残り、`CARGO_BIN_EXE_*` / `CARGO_MANIFEST_DIR` を使う実バイナリのテストが「No such file」で全滅することがある(実験で確認済み)。`mv` 自体は問題なく、`Cargo.toml` / `Cargo.lock` に絶対パスは無い。インストール済みのバイナリは独立したファイルなので影響を受けない。
- インストール履歴は `~/.local/share/cargo/.crates2.json` にソースの絶対パスで残る。移動後に `just install` を実行すると新しい場所で上書きされる。
- ユーザーの dotfiles 側は、`.zshrc` に `eval "$(flatdir shell)"` を書くだけ。dotfiles に flatdir のコードや関数の文面は残していない。
