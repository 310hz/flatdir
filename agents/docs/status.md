# status

## 現在の状態

- 独立したリポジトリとして整備済み(dotfiles からは切り離した。経緯は [concepts/history.md](../concepts/history.md))。
- sh 版を Rust で再実装し、選択・アーカイブ・復元・削除を 1 つの TUI に統合済み([concepts/tui.md](../concepts/tui.md))。作成・clone も TUI の New ビュー(`Ctrl+N`)に統合し、残るサブコマンドは root/status/shell。`just check` が通る。
- `Ctrl+O` で現在行を Finder で開く。`open` / `xdg-open` は任意機能のための外部呼び出し(`Opener` の裏にあり、テストは偽物)。
- 外部コマンド依存を廃止: fzf・skim→自前 TUI、trash→`trash` crate、rich/eza→内蔵。`gh`・`git`・`curl`・`git-lfs`・`zoxide` は任意機能のため外部のまま。
- ユーザー向けの文言(ヘルプ・メッセージ・README・コード内コメント)は英語。`agents/` 配下のドキュメントは日本語。
- 旧版との差分(ユーザー指示による設計変更):
  - 管理 root は 1 つだけ。複数 root は廃止。
  - アーカイブの目印ファイル `.flatdir_archived` は廃止。root 直下に相当する `<archive_root>/<HOME からの相対パス>` の直下全体がアーカイブ。旧版で作られたアーカイブはそのまま使え、`restore` 時に旧マーカーを取り除く。
  - 設定は `~/.config/flatdir/config.toml`(`root`、`sound`)。`color` は skim と一緒に廃止。旧版の `src/flatdir/config`(`DIRS=`)は読まない。

## 未解決事項

- `feat/tui-create` ブランチで New ビュー(作成・clone の TUI 統合)を実装した。main には未マージ。ユーザーが実機で試してからマージする。
- 今後の予定: Python(`uv init` 済み)などの雛形を New ビューの種類に足す。足し方は [concepts/tui.md](concepts/tui.md) の「拡張」。

- 未公開(GitHub 未アップロード)。公開後は `cargo install --git <URL>` で入る想定。
- `~/.config/flatdir/config`(2025-12 作成、旧版の遺物と思われる)が実環境に存在する。新版は `config.toml` を使うため干渉しないが、中身は未確認。
- New ビューの Git (private) と GitHub 側の clone は、`gh` / `git` を外部実行するため自動テストの対象外(`FakeForge` で手順の呼び分けだけ検証)。検索と HF の clone は、開発時に実環境で一度確認済み。
- ユーザーの仕様のうち、次の解釈はユーザーの確認待ち: (1) マークが無ければ現在行が対象。(2) 効果音は設定 `sound` でオプトイン(Ctrl+R のゴミ箱でのみ鳴る)。(3) アーカイブ時の `node_modules` 等の削除確認は廃止し、残存を報告するだけ。
- 「一覧の左端に日本語が出る」不具合は、文字幅の食い違い(`⚠️` 等)が原因と推定して対策済み([concepts/tui.md](../concepts/tui.md))。実機での確認待ち。再発したら、そのときのディレクトリ名と README の該当部分を教えてもらう。
- テスト(`tests/cli.rs`)の注意: macOS は新しく作った実行ファイルの初回起動を検査して数百ミリ秒遅れる。偽の `zoxide`/`open` を書いた直後に `warm_up` で 1 回実行してから使うこと(並列実行で zoxide の並び替えテストが落ちる原因だった)。
- プレビューは同期生成のため、巨大ディレクトリで描画が詰まる可能性がある(別スレッド化は未実施)。
- Enter で `cd` するにはシェル関数が必要(プログラムは親シェルのディレクトリを変えられない)。`flatdir shell [zsh|bash]` が関数を出力し、`eval "$(flatdir shell)"` で入る(2 つの名前は同じ出力)。ユーザーの dotfiles 側は `.zshrc` に `eval "$(flatdir shell)"` を書くだけで、関数の文面は持たない。関数が無いまま Enter を押すと、案内を stderr に出す。
