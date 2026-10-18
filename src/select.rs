use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use skim::prelude::*;

use crate::context::Context;
use crate::preview;

/// Only ANSI palette indexes (0-15), reverse video and bold, so it follows the terminal theme and
/// reads the same on light and dark backgrounds. skim's own default is a dark-background 256-color scheme.
const DEFAULT_COLOR: &str = "none,matched:2+b,current:-1+r,current_match:2+b+r,prompt:4,cursor:1,\
selected:5,header:6,info:-1+d,spinner:2,border:-1,scrollbar:-1";

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
}

pub trait Selector {
    /// Returns the selected lines verbatim. Empty on cancel.
    /// A line is `display\tvalue...`; only the first field is shown.
    fn select(&self, lines: &[String], opts: &SelectOpts) -> Result<Vec<String>>;
}

/// skim used as a library. Unlike the `sk`/`fzf` binaries it reads no `*_DEFAULT_OPTIONS`
/// environment variable, so nothing in the environment can turn the picker non-interactive.
pub struct SkimSelector {
    pub color: Option<String>,
}

struct Row {
    line: String,
    preview: Option<Preview>,
}

impl Row {
    fn field(&self, n: usize) -> &str {
        self.line.split('\t').nth(n).unwrap_or("")
    }
}

impl SkimItem for Row {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.field(0))
    }

    fn output(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.line)
    }

    fn preview(&self, context: PreviewContext) -> ItemPreview {
        match self.preview {
            Some(Preview::Dir) => {
                ItemPreview::AnsiText(preview::dir(self.field(1).as_ref(), context.width))
            }
            Some(Preview::Clone) => {
                ItemPreview::AnsiText(preview::repo(self.field(0), context.width))
            }
            None => ItemPreview::Text(String::new()),
        }
    }
}

impl Selector for SkimSelector {
    fn select(&self, lines: &[String], opts: &SelectOpts) -> Result<Vec<String>> {
        let mut builder = SkimOptionsBuilder::default();
        builder
            .height(format!("{}%", opts.height_percent))
            .reverse(true)
            .multi(opts.multi)
            .color(
                self.color
                    .clone()
                    .unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            );
        if let Some(prompt) = &opts.prompt {
            builder.prompt(prompt.clone());
        }
        if opts.multi {
            builder.header("Tab: select multiple  Enter: confirm");
        }
        if opts.preview.is_some() {
            builder.preview("");
        }
        let options = builder
            .build()
            .map_err(|e| anyhow!("invalid skim options: {e}"))?;

        let rows = lines.iter().map(|line| Row {
            line: line.clone(),
            preview: opts.preview,
        });
        let output = Skim::run_items(options, rows).map_err(|e| anyhow!("picker failed: {e}"))?;
        if output.is_abort {
            return Ok(Vec::new());
        }
        Ok(output
            .selected_items
            .iter()
            .map(|m| m.output().into_owned())
            .collect())
    }
}

/// Lets the user pick rows and returns their indices.
/// Fails on any row that was not offered, so a mismatched selection can never act on something else.
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
