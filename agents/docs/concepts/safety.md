# 安全設計

## 経緯

旧 sh 版の高速化中、計測のために `FZF_DEFAULT_OPTS="--filter=" flatdir restore` と `remove` を実行した。`--filter=` の fzf は対話せず全件を出力するため、全アーカイブの解除と全プロジェクトの `trash` 移動が起きた(ユーザーの `rm` エイリアスが `trash` だったため復旧できた)。原因は言語ではなく設計: fzf の出力をそのまま移動・削除へ流し、確認も実行条件もなかった。

## ガード(`src/guard.rs` ほか)

1. **対話端末必須**: `remove` / `archive` / `restore` と、`clone` の上書きは stdin・stderr が端末でなければ、fzf を起動する前に拒否。環境変数では回避できない(テストは `Context.interactive` に値を注入する)。
2. **全件選択の拒否**: 候補が 2 件以上で全て選ばれたら拒否。通常の操作では起きない状況だけを止めるので、毎回の確認は不要。
3. **fzf 環境の検査**: `FZF_DEFAULT_OPTS` に `--filter`・`-1`・`-0` 系があれば無視して警告。普通の配色設定は引き継ぐ。`FZF_DEFAULT_OPTS_FILE` は検査できないので常に捨てる。`config.toml` の `fzf_opts` も同じ禁止リストで拒否。
4. **選択結果の検証**: fzf が返した行は、提示した行と完全一致するものだけを受け付ける。
5. **2 段階実行**: 全対象を検証(直下 1 階層か・通常ディレクトリか・移動先が空か)してから実行し、途中失敗で半端な状態を残さない。
6. **削除は trash のみ**: `trash` が無ければ `rm` にフォールバックせずエラー。`init`・`clone` の名前は root 直下 1 階層に限る(`..` や `/` を拒否)。

## テスト方針

実 HOME・実 fzf・実 trash に触れない。`tests/commands.rs` は使い捨てディレクトリと偽の selector/trasher、`tests/cli.rs` は `env_clear` した実バイナリと偽の `fzf`/`trash`。事故の再現(非対話 + `--filter=`)は `incident_regression_*` で固定している。
