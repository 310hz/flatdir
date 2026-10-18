use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};

use crate::config::fzf_env_is_safe;
use crate::context::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preview {
    Dir,
    Clone,
}

#[derive(Debug, Clone)]
pub struct SelectOpts {
    pub multi: bool,
    pub prompt: Option<String>,
    pub height_percent: u8,
    pub preview: Option<Preview>,
}

impl SelectOpts {
    pub fn dirs(multi: bool) -> Self {
        Self {
            multi,
            prompt: None,
            height_percent: 60,
            preview: Some(Preview::Dir),
        }
    }

    pub fn menu(prompt: &str) -> Self {
        Self {
            multi: false,
            prompt: Some(prompt.to_string()),
            height_percent: 40,
            preview: None,
        }
    }
}

pub trait Selector {
    /// 選択された行をそのまま返す。キャンセルや該当なしは空。
    fn select(&self, lines: &[String], opts: &SelectOpts) -> Result<Vec<String>>;
}

pub struct Fzf {
    pub extra_opts: Vec<String>,
    pub exe: PathBuf,
}

impl Selector for Fzf {
    fn select(&self, lines: &[String], opts: &SelectOpts) -> Result<Vec<String>> {
        let mut cmd = Command::new("fzf");
        // `--filter=` などが環境変数経由で効くと非対話・全件通過になり、破壊的処理が全対象に走る。
        // 配色などの普通の設定は引き継ぎ、危険なオプションを含むときだけ捨てる。
        // ファイル経由の設定は中身を検査できないので常に捨てる。必要な設定は config の fzf_opts へ。
        cmd.env_remove("FZF_DEFAULT_OPTS_FILE");
        if let Ok(value) = std::env::var("FZF_DEFAULT_OPTS")
            && !fzf_env_is_safe(&value)
        {
            eprintln!("warning: FZF_DEFAULT_OPTS に危険なオプションを含むため無視する");
            cmd.env_remove("FZF_DEFAULT_OPTS");
        }
        cmd.args(&self.extra_opts);
        cmd.arg(format!("--height={}%", opts.height_percent))
            .arg("--reverse")
            .arg("--delimiter=\t")
            .arg("--with-nth=1");
        if let Some(prompt) = &opts.prompt {
            cmd.arg(format!("--prompt={prompt}"));
        }
        if let Some(preview) = opts.preview {
            // 行は `表示\tパス`(ディレクトリ)か `owner/repo\t星数`(clone)。参照する列が違う。
            let (sub, field) = match preview {
                Preview::Dir => ("__preview", 2),
                Preview::Clone => ("__clone-preview", 1),
            };
            cmd.arg(format!(
                "--preview={} {sub} {{{field}}}",
                shell_quote(&self.exe.to_string_lossy())
            ));
        }
        if opts.multi {
            cmd.args([
                "--multi",
                "--bind=tab:toggle+down,shift-tab:toggle+up",
                "--header=Tab: select multiple  Enter: confirm",
            ]);
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("required command not found: fzf")?;
        let mut stdin = child.stdin.take().context("failed to open fzf stdin")?;
        let input = lines.join("\n") + "\n";
        let writer = std::thread::spawn(move || {
            // fzf が先に終了した場合の EPIPE は無視してよい。
            let _ = stdin.write_all(input.as_bytes());
        });
        let output = child.wait_with_output()?;
        let _ = writer.join();

        match output.status.code() {
            Some(0) => Ok(String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()),
            Some(1 | 130) => Ok(Vec::new()),
            _ => bail!("fzf failed: {}", output.status),
        }
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// 行を選択させ、元の行の添字で返す。
/// 提示していない行が返ってきたら即エラーにして、選択結果の取り違えで別物を操作しないようにする。
pub fn pick(ctx: &Context, rows: &[Vec<String>], opts: &SelectOpts) -> Result<Vec<usize>> {
    let lines: Vec<String> = rows.iter().map(|r| r.join("\t")).collect();
    let index: HashMap<&str, usize> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| (l.as_str(), i))
        .collect();

    let mut picked = Vec::new();
    for line in ctx.selector.select(&lines, opts)? {
        let Some(&i) = index.get(line.as_str()) else {
            bail!("selector returned an unknown row: {line}");
        };
        if !picked.contains(&i) {
            picked.push(i);
        }
    }
    Ok(picked)
}
