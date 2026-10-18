# status

## 現在の状態

- sh 版を Rust で再実装し、選択・アーカイブ・復元・削除を 1 つの TUI に統合済み([concepts/tui.md](../concepts/tui.md))。残るサブコマンドは track/untrack/status/init/clone。`just check` が通る。
- 外部コマンド依存を廃止: fzf→自前 TUI(`clone -s` のみ skim)、trash→`trash` crate、rich/eza→内蔵。`gh`・`git`・`zoxide` は任意機能のため外部のまま。
- ユーザー向けの文言(ヘルプ・メッセージ・README・コード内コメント)は英語。`agents/` 配下のドキュメントは日本語。
- 旧版との差分(ユーザー指示による設計変更):
  - 管理 root は 1 つだけ。複数 root は廃止。
  - アーカイブの目印ファイル `.flatdir_archived` は廃止。root 直下に相当する `<archive_root>/<HOME からの相対パス>` の直下全体がアーカイブ。旧版で作られたアーカイブはそのまま使え、`restore` 時に旧マーカーを取り除く。
  - 設定は `~/.config/flatdir/config.toml`(`root`、`color`)。旧版の `src/flatdir/config`(`DIRS=`)は読まない。

## 未解決事項

- dotfiles への組み込み(`modules/flatdir.sh` を新バイナリ呼び出しに差し替える)は未実施。ユーザーからの依頼待ち。
- 未公開(GitHub 未アップロード)。公開後は `cargo install --git <URL>` で入る想定。
- `~/.config/flatdir/config`(2025-12 作成、旧版の遺物と思われる)が実環境に存在する。新版は `config.toml` を使うため干渉しないが、中身は未確認。
- `init --git` / `clone` は `gh` / `git` を外部実行するため自動テストの対象外。
- ユーザーの仕様のうち、次の解釈はユーザーの確認待ち: (1) Ctrl+R は「現在のアクションを実行」、Ctrl+T は「Archive/Restore ⇄ Remove の切り替え」で、既定は安全側・実行後は必ず戻る。(2) マークが無ければ現在行が対象。(3) 効果音は設定 `sound` でオプトイン。(4) アーカイブ時の `node_modules` 等の削除確認は廃止し、残存を報告するだけ。
- プレビューは同期生成のため、巨大ディレクトリで描画が詰まる可能性がある(別スレッド化は未実施)。
- `clone -s` だけ skim(ピッカー)に依存している。自前 TUI のリスト部品を汎用化すれば skim は外せる。
- dotfiles の `modules/flatdir.sh` は旧 `-a` を前提にしている。組み込み時に直す。
