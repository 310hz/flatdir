# status

## 現在の状態

- 主な機能は実装済み。TUI で、一覧(Active / Archived)からの `cd`・アーカイブ・復元・ゴミ箱、New ビューでの作成・clone(GitHub / Hugging Face)ができる。サブコマンドは `root`・`status`・`shell` だけ。`just check` が通る。
- GitHub で公開済み(`https://github.com/310hz/flatdir`、public)。`cargo install --git` で入る。crates.io には出さない。
- ユーザー向けの文言(TUI・メッセージ・README・コード内コメント)は英語。`agents/` のドキュメントは日本語。
- README は最小限にしている。キー操作は TUI の `?` と [spec/tui.md](spec/tui.md) に任せ、README にはスクリーンショットをユーザーが貼る予定。

## 未解決事項

- アーカイブを「root に置いたまま状態ファイルで隠す」方式に変えた。ユーザーの実環境の旧アーカイブ 38 件は root へ移し、状態ファイルに記録済み(日時は旧目印の更新日時、無ければディレクトリの ctime)。実機の TUI での確認待ち。
- New ビューの Git (private) と、SSH での GitHub clone は、実環境で動かしていない(GitHub にリポジトリができるため)。テストは `FakeForge` で手順の呼び分けだけ見ている。検索と Hugging Face の clone は実環境で確認済み。
- 次の解釈はユーザーの確認待ち: (1) マークが無ければ現在行が対象。(2) 効果音は設定 `sound` でオプトイン。(3) アーカイブ時に `node_modules` 等を削除せず、残りを報告するだけ。
- 「一覧の左端に日本語が出る」不具合は、文字幅の食い違いが原因と推定して対策済み([concepts/tui.md](concepts/tui.md))。実機での確認待ち。再発したら、そのときのディレクトリ名と README の該当部分をもらう。
- 実環境に `~/.config/flatdir/config`(旧版の遺物と思われる)がある。今の版は `config.toml` を読むので干渉しない。中身は未確認。

## 次にやること

- New ビューに雛形の種類を足す(例: `uv init` 済みの Python プロジェクト)。足し方は [concepts/tui.md](concepts/tui.md) の「New ビュー」。
