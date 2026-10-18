//! A small Markdown renderer for the preview pane. It uses only text attributes and the terminal's
//! own ANSI palette, so it reads the same on light and dark backgrounds. Wrapping is left to the widget.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const MAX_LINES: usize = 400;

struct Renderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    styles: Vec<Style>,
    lists: Vec<Option<u64>>,
    quote_depth: usize,
    in_code_block: bool,
}

pub fn render(md: &str) -> Vec<Line<'static>> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut r = Renderer {
        lines: Vec::new(),
        spans: Vec::new(),
        styles: vec![Style::default()],
        lists: Vec::new(),
        quote_depth: 0,
        in_code_block: false,
    };
    for event in Parser::new_ext(md, options) {
        r.event(event);
        if r.lines.len() >= MAX_LINES {
            break;
        }
    }
    r.flush();
    while r.lines.last().is_some_and(|l| l.spans.is_empty()) {
        r.lines.pop();
    }
    r.lines
}

impl Renderer {
    fn style(&self) -> Style {
        *self.styles.last().unwrap_or(&Style::default())
    }

    fn push_style(&mut self, f: impl FnOnce(Style) -> Style) {
        let next = f(self.style());
        self.styles.push(next);
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    fn text(&mut self, text: &str) {
        let style = self.style();
        self.spans.push(Span::styled(text.to_string(), style));
    }

    fn flush(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        let mut spans = Vec::new();
        if self.quote_depth > 0 {
            spans.push(Span::styled(
                "│ ".repeat(self.quote_depth),
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        spans.append(&mut self.spans);
        self.lines.push(Line::from(spans));
    }

    fn blank(&mut self) {
        self.flush();
        if self.lines.last().is_some_and(|l| !l.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) if self.in_code_block => {
                for line in t.lines() {
                    self.spans
                        .push(Span::styled(format!("  {line}"), self.style()));
                    self.flush();
                }
            }
            Event::Text(t) => self.text(&t),
            Event::Code(t) => {
                let style = self.style().fg(Color::Cyan);
                self.spans.push(Span::styled(t.to_string(), style));
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.blank();
                self.lines.push(Line::from(Span::styled(
                    "────────",
                    Style::default().add_modifier(Modifier::DIM),
                )));
                self.lines.push(Line::default());
            }
            Event::TaskListMarker(done) => self.text(if done { "[x] " } else { "[ ] " }),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.blank();
                self.push_style(|s| {
                    let s = s.add_modifier(Modifier::BOLD);
                    if level == HeadingLevel::H1 {
                        s.add_modifier(Modifier::UNDERLINED)
                    } else {
                        s
                    }
                });
            }
            Tag::Emphasis => self.push_style(|s| s.add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(|s| s.add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => self.push_style(|s| s.add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { .. } => self.push_style(|s| s.add_modifier(Modifier::UNDERLINED)),
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                self.blank();
                self.in_code_block = true;
                self.push_style(|s| s.add_modifier(Modifier::DIM));
                if let CodeBlockKind::Fenced(lang) = kind
                    && !lang.is_empty()
                {
                    self.spans.push(Span::styled(
                        format!("  {lang}"),
                        Style::default().add_modifier(Modifier::DIM | Modifier::ITALIC),
                    ));
                    self.flush();
                }
            }
            Tag::List(first) => {
                self.flush();
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush();
                let indent = "  ".repeat(self.lists.len().saturating_sub(1));
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => "• ".to_string(),
                };
                self.spans.push(Span::raw(format!("{indent}{marker}")));
            }
            Tag::TableCell => {
                if !self.spans.is_empty() {
                    self.spans.push(Span::styled(
                        " │ ",
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
            }
            Tag::TableHead => self.push_style(|s| s.add_modifier(Modifier::BOLD)),
            Tag::Image { .. } => self.text("[image: "),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.flush();
                self.pop_style();
                self.lines.push(Line::default());
            }
            TagEnd::Paragraph => self.blank(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.pop_style()
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.blank();
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.pop_style();
                self.blank();
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            TagEnd::Item | TagEnd::TableRow => self.flush(),
            TagEnd::TableHead => {
                self.flush();
                self.pop_style();
            }
            TagEnd::Table => self.blank(),
            TagEnd::Image => self.text("]"),
            _ => {}
        }
    }
}
