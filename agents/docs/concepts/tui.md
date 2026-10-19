# TUI

`flatdir`(引数なし)で開く、ほぼ全機能を集約した画面。ratatui(stderr に描画)+ crossterm。

## キー

| キー | 動作 |
|---|---|
| 文字入力 | ファジー絞り込み(nucleo-matcher)。`Ctrl+U` で消去。`?` はヘルプに取られるので入力できない |
| `Enter` | 現在行のパスを出力して終了(Active ならプロジェクト、Archived ならアーカイブ先) |
| `Tab` / `Shift+Tab` | 現在行にマークを付け外しして下/上へ |
| `Ctrl+Q` | すべてのマークを解除(カーソルや一覧はそのまま。マークが無ければ何もしない) |
| `Ctrl+T` | Active / Archived の切り替え。マークとアクションはリセット |
| `Ctrl+A` | マーク済み(無ければ現在行)のアーカイブ状態を反転: Active ではアーカイブ、Archived では復元 |
| `Ctrl+R` | マーク済み(無ければ現在行)をゴミ箱へ(両方の一覧で有効) |
| `Ctrl+O` | 現在行のディレクトリをファイルマネージャで開く(macOS は `open`=Finder、他は `xdg-open`)。何も変更しない |
| `Ctrl+N` | New ビュー(下記)を開く / 閉じる |
| `Ctrl+P` | プレビュー切り替え: README(Markdown)⇄ ツリー。README が無ければツリーに自動フォールバック |
| 左クリック | 一覧の行の上: カーソルをその行へ |
| 右クリック | 一覧の行の上: その行のマークを付け外し(Tab と同じだが、カーソルは動かさない) |
| マウスホイール | 一覧の上: カーソル移動(1行/ノッチ)。プレビューの上: プレビューをスクロール(3行/ノッチ)。それ以外の場所は何もしない |
| `?` | ヘルプ(ショートカット一覧)を手前に開く。開いている間は `?` / `Esc` で閉じ、`Ctrl+C` で終了。それ以外のキー・マウス・貼り付けは捨てる |
| `Esc` / `Ctrl+C` | 終了 |

Ctrl+R(ゴミ箱)と Ctrl+A(アーカイブ/復元)は、どちらもモード無しの 1 キー操作。当初「Ctrl+R で実行、Ctrl+T で Archive⇄Remove のモード切替」と解釈して実装したが、ユーザーの意図と違ったため改めた。その後、頭文字に合わせて一覧の切り替え(Toggle)を Ctrl+T、アーカイブ/復元(Archive)を Ctrl+A に入れ替えた(ユーザー指示)。色は使わない(エラー行の赤とマークの色のみ)。

## 構成と一般化の方針

- `tui/app.rs`: 端末に依存しない状態機械(`on_key` → `Outcome`)。テストはキーイベントを直接流す。
- `tui/ui.rs`: 描画のみ。`TestBackend` で画面と色を検査できる。
- `tui/{preview,markdown,tree}.rs`: プレビュー生成。Markdown は用語を最小限にした自前描画で、色は ANSI パレット+属性のみ。
- `create.rs`: New ビューの中身。種類(`Recipe`)、入力から Enter の動作を決める `plan`、外部コマンドを使う仕事(`Job`)。外部コマンド自体は `services.rs` の `Forge`(本物は `SystemForge`、テストは `FakeForge`)。
- `ops.rs`: アーカイブ・復元・削除の実行層。選択や確認とは独立しており、検証(対象が直下 1 階層か・通常ディレクトリか・移動先が空か)を全対象に行ってから実行する。新しい操作はここに足し、TUI のアクションとして繋ぐ。
- 件数ガード(全件選択の拒否)は `App::execute` が `ops` の前に必ず通す。操作を足す場合も同じ経路に載せる。

## 起動速度

最初のフレームは、ディレクトリ直下の読み取りだけで描く(約 3ms)。次の処理は待たない:

- zoxide のスコア: 別スレッドで取得し、届いたら並び替える(カーソル位置のエントリは維持)。
- アーカイブ一覧: `Ctrl+T` を最初に押したときに読む。
- プレビュー: 現在行の分だけを、描画時に生成して 1 件キャッシュする。

既知の限界: プレビューは同期生成。巨大なディレクトリでは描画が詰まる可能性がある(ツリーは深さ 3・300 件で打ち切り)。必要になったら別スレッド化する。

## 安全面の補足

- 貼り付け(bracketed paste)は文字データとしてだけ扱い、制御文字は捨てる。ショートカットを貼り付けで発火させない。
- TUI 内ではアーカイブ後の `node_modules` 等の削除確認をしない(`/dev/tty` の対話が TUI と衝突する)。残っているものを状態行に報告するだけで、自動削除はしない。

## 画面崩れ対策(文字幅)

ratatui と端末で文字幅の解釈が割れると、その行の残りがずれ、行末の文字が自動折り返しで次の行の左端に出る(日本語が一覧の左端に出る不具合として発生した)。

- 原因の候補は、`⚠️`(U+26A0 + 異体字選択子 U+FE0F)のような「幅1の基底文字 + VS16」。端末ごとに幅が割れる。実際のユーザーの README に含まれていた。実機の端末でのピクセルレベルの再現はできていない。
- 対策(1): `tui/text.rs` の `sanitize` で、U+FE0E/FE0F、ゼロ幅文字(U+200B〜200D、U+2060)、BOM を表示前に除く。タブは 4 スペースに展開する(ratatui は制御文字を捨てるため、そのままだと字下げが消える)。プレビュー、ツリー、一覧、状態行が対象。
- 対策(2): TUI の間だけ端末の自動折り返し(DECAWM)を切る(`\x1b[?7l`、終了時と panic 時に `\x1b[?7h`)。幅が食い違っても、溢れた文字が次の行の左端に出ない。
- 画面の構造に使う記号(`│`、`▌` 等の East Asian Ambiguous)は、端末が「曖昧幅を全角扱い」にしていると全行がずれる。その設定では「稀に」ではなく毎回崩れるため、今回の症状とは別。必要になったら設定で ASCII 記号に切り替える。
- 回帰テスト: `width_ambiguous_invisible_characters_never_reach_the_screen`(`tests/tui.rs`)、`autowrap_is_disabled_only_while_the_tui_runs`(`tests/cli.rs`)。
- 検証メモ: `pyte` は幅2文字を上書きした後の空き桁の扱いが実際の端末と違い、偽陽性が出るので、画面崩れの判定には使えない。

## マウス

- 起動時にマウスキャプチャを有効にし、終了時と panic 時に解除する(`TerminalGuard`)。ホイールの処理先は、直近の描画で記録したペインの矩形(`App::panes`)で決める。ペインの割合を変えてもそのまま追従する。
- キャプチャ中は端末側のマウス選択が効かない。Shift(端末によっては Option)を押しながらなら選べる。この制約は README に書いてある。
- crossterm は押していないボタンの移動イベントまで要求するので、`on_mouse` は再描画が必要なときだけ true を返す。移動のたびに再描画しない。
- プレビューのスクロール量は描画時に折り返し後の行数で上限を掛ける(`Paragraph::line_count`、ratatui の `unstable-rendered-line-info` 機能)。別のエントリやモードに変わると 0 に戻る。

## タイトル行とヘルプ

ショートカットの説明は、以前は画面上部に常時並べていたが、項目が増えて数行を占めるようになったため、`?` で開くヘルプウィンドウに移した(ユーザー指示)。キーを `?` にしたのは、ヘルプキーとして定番で、ディレクトリ名に `?` が入ることはまず無いから。`Ctrl+H` も候補だったが、端末によっては Backspace と同じ 0x08 を送るため採らなかった。

- 縦の並びは「タイトル行 / 空き行 / クエリ行 / 一覧 / ステータス行」。タイトル行は左に `flatdir` と Active / Archived のタブ・件数、右端に `? Help`。プログラム名とタブの間は `TITLE_GAP`(4 桁)空ける。
- 件数と「N marked」は固定幅で確保し、モードやマークでタイトル行の項目が動かないようにする。
- ヘルプは画面中央に重ねる枠(`Clear` で下を消してから描く)。開いている間は、隠れた一覧への操作(特に Ctrl+R / Ctrl+A)が誤って走らないよう、閉じる・終了以外の入力をすべて捨てる。
- ヘルプの説明は Title Case の動詞句に揃える(macOS のメニュー項目の慣例。例: `Move to Trash`、`Open in Finder`。Linux では `Open in File Manager`)。例外は `Enter` の `cd`(コマンド名なので小文字のまま)。切り替え系は `Toggle List: Active/Archived` のように「対象: 候補」の形にする(プレビューの候補名は、ペインの見出しと同じ `README`/`Tree`)。`Ctrl+A` だけは今の一覧でする動作(`Archive`/`Restore`)を出す。
- 実端末テスト(`tests/cli.rs`)のマウス座標は `FIRST_LIST_ROW` 1 か所に集約している。タイトル周りの行数を変えたらこの定数を直す。
- 回帰テスト: `the_title_keeps_its_layout_when_the_mode_changes`、`the_title_has_room_before_the_tabs_and_a_blank_row_above_the_content`、`help_takes_the_keys_until_it_is_closed`。

## New ビュー(作成と clone)

`init`・`clone` サブコマンドを TUI に統合したもの(ユーザー指示)。別ウィンドウにはせず、入力欄はそのまま使い、一覧とプレビューの場所に選択肢を出す。

- `Ctrl+N` で開閉。入力中の文字は名前としてそのまま引き継ぐ。`Esc` も閉じる(このビューでは終了ではない)。タイトル行の `New` タブが反転し、プロンプトは `+ `。
- 種類は `create::RECIPES` の順に Tab / Shift+Tab で選ぶ: Directory、Git (private)、Clone: GitHub / HF Model / HF Dataset / HF Space。
- Enter の動作は入力で決まり(`create::plan`)、その内容を Enter の前に画面に出す(エラーなら赤で理由)。
  - Directory: `<root>/<name>` を作る(同期)。
  - Git (private): `gh repo create --private` → clone → 空の README で初回コミットと push(旧 `init --git` と同じ手順)。
  - Clone: URL か `owner/name` ならそのまま clone。`/` を含まなければ検索し、結果を ↑↓(または j / k)で選んで Enter で clone。結果が出ている間は名前と種類を編集できない(入力は薄字になり、カーソルも消える)。Esc で結果を消すと編集に戻り、もう一度 Esc で一覧へ。検索中も編集はできない。以前は「文字を変えると結果が消え、元に戻すと復活する」作りだったが、直感的でないとユーザーが判断し、このロック方式を選んだ(他の案: 古い結果を薄く残す、打つそばから検索する)。
- 検索は GitHub が `gh search repos --match=name`、Hugging Face は公開 API(`https://huggingface.co/api/{models,datasets,spaces}?search=`、認証不要)を curl で叩く。どちらも 30 件、HF は likes 順。
- LFS: HF の clone のときだけ `Ctrl+L` で切り替え。既定はスキップ(`GIT_LFS_SKIP_SMUDGE=1`。ポインタだけ置く)。モデルの重みは数十 GB になり得るため(ユーザー指示で選択式に)。
- 行き先が root かアーカイブに既にあれば拒否。上書きはしない。
- 外部コマンドは別スレッドで実行し(`App::start` → `poll_job`)、同時に 1 つだけ。出力は捕まえて、失敗時は stderr の最終行を状態行に出す。git はプロンプトを出せないので `GIT_TERMINAL_PROMPT=0`、`GIT_SSH_COMMAND` が未設定なら `ssh -o BatchMode=yes`(パスフレーズ入力で固まらずに失敗させる)。
- 終わると Active 一覧に戻り、カーソルは作ったディレクトリ。そのまま Enter で cd できる。
- New ビューでは一覧の操作(Ctrl+R / Ctrl+A / Ctrl+O / Ctrl+T / Ctrl+P / Ctrl+Q)とマウスを無視する。隠れた一覧への誤操作を防ぐため。`?` のヘルプは New ビュー用の項目を出す。

### 拡張

種類を足すとき(例: `uv init` 済みの Python プロジェクト)は、`Recipe` に variant を足し、`label`・`hint`・`RECIPES`・`plan`(入力から何をするか)を埋め、外部コマンドが要るなら `Job` と `Forge` に手順を足す。Git 化と組み合わせたくなったら、`Recipe` を「土台(空 / Git)× 雛形(なし / Python …)」の 2 軸に分ける。

### キーについて

最初の指示は `Ctrl+I` で開いて `Ctrl+Enter` で作成だったが、端末では `Ctrl+I` は Tab(0x09)と同じで、`Ctrl+Enter` も多くの端末で Enter と区別できない(kitty keyboard protocol 対応端末のみ)。ユーザーと相談して `Ctrl+N`(New)と Enter にした。

## Enter で cd する仕組み

プログラムは親シェルのカレントディレクトリを変えられないので、`flatdir` は選んだパスを stdout に出すだけ。シェル関数(`src/shell.rs` の `SHELL_FUNCTION`、`flatdir shell` で出力)が出力を `$(...)` で受け取って `cd` する。

- `cd` するのは、引数なし(TUI)と `root --cd` のときだけ。`root`(引数なし)や `status` などの出力はそのまま表示する。どの呼び出しで `cd` するかは、関数側で引数を見て決める(バイナリの出力形式は変えない。出力に目印を付けると、関数を使わない人の画面に目印が出てしまう)。

- 関数が無いと「パスが表示されるだけ」になる。以前、dotfiles 側の関数をコメントアウトしたまま気づかず、`cd` が動かない状態が続いた。
- 再発防止: stdout が端末のとき(= 関数に受け取られていない)は、案内を stderr に出す。関数経由なら stdout はパイプなので出ない。
- 検証: `tests/cli.rs` の `enter_changes_the_directory_in_zsh` / `_in_bash` と `root_cd_changes_the_directory_in_zsh` / `_in_bash` が、実際のシェルを pty で動かして `cd` を確かめる。README の関数と `flatdir shell` の出力が同一であることもテストしている。
