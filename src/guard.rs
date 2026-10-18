use std::fs;
use std::path::Path;

use anyhow::{Result, bail};

use crate::context::Context;

/// 非対話(パイプ・リダイレクト・自動実行)では破壊的コマンドを動かさない。
pub fn require_interactive(ctx: &Context, command: &str) -> Result<()> {
    if !ctx.interactive {
        bail!("{command} は対話端末でのみ実行できる(stdin/stderr が端末ではない)");
    }
    Ok(())
}

/// 候補を全件選んだ場合は拒否する。非対話 fzf の全件通過のような事故の最後の砦。
pub fn reject_select_all(total: usize, selected: usize) -> Result<()> {
    if total >= 2 && selected >= total {
        bail!("全件選択は拒否した({selected}/{total})。意図した操作なら対象を減らして実行する");
    }
    Ok(())
}

/// 操作対象は管理領域の直下 1 階層に限る。
pub fn ensure_child_of(parent: &Path, path: &Path) -> Result<()> {
    let name_ok = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| !n.is_empty() && !n.starts_with('.'));
    if path.parent() != Some(parent) || !name_ok {
        bail!(
            "{} は {} 直下のディレクトリではない",
            path.display(),
            parent.display()
        );
    }
    Ok(())
}

pub fn ensure_plain_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => Ok(()),
        _ => bail!("not a directory: {}", path.display()),
    }
}
