# status

## 現在の状態

- sh 版のコマンド(選択・`-a`・track/untrack・remove/archive/restore・init・clone)を Rust で再実装済み。`just check` が通る。`list` は管理 root が 1 つなので廃止。
- ユーザー向けの文言(ヘルプ・メッセージ・README・コード内コメント)は英語。`agents/` 配下のドキュメントは日本語。
- 旧版との差分(ユーザー指示による設計変更):
  - 管理 root は 1 つだけ。複数 root は廃止。
  - アーカイブの目印ファイル `.flatdir_archived` は廃止。root 直下に相当する `<archive_root>/<HOME からの相対パス>` の直下全体がアーカイブ。旧版で作られたアーカイブはそのまま使え、`restore` 時に旧マーカーを取り除く。
  - 設定は `~/.config/flatdir/config.toml`(`root`、`fzf_opts`)。旧版の `src/flatdir/config`(`DIRS=`)は読まない。

## 未解決事項

- dotfiles への組み込み(`modules/flatdir.sh` を新バイナリ呼び出しに差し替える)は未実施。ユーザーからの依頼待ち。
- 未公開(GitHub 未アップロード)。公開後は `cargo install --git <URL>` で入る想定。
- `~/.config/flatdir/config`(2025-12 作成、旧版の遺物と思われる)が実環境に存在する。新版は `config.toml` を使うため干渉しないが、中身は未確認。
- `init --git` / `clone` は `gh` / `git` を外部実行するため自動テストの対象外。
