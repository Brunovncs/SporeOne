//! A README's Markdown, drawn on a mod's page: headings, paragraphs, lists, code, quotes, rules
//! and tables as rows of text. Images are left out, but for the text of one that is a link;
//! badges are left out entirely, and raw HTML keeps only its text.

use crate::theme::{MONO_FONT, Theme, text};
use gpui::{
    AnyElement, Div, FontStyle, FontWeight, HighlightStyle, InteractiveText, IntoElement, ParentElement, SharedString, StrikethroughStyle,
    Styled, StyledText, UnderlineStyle, div, px,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// How deep quotes and lists may nest; deeper ones are drawn at this depth.
const MAX_DEPTH: usize = 12;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Marks {
    pub strong: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: bool,
}

/// A run of text with its marks and links, by byte range.
#[derive(Clone, Default, Debug)]
pub struct Inline {
    pub text: String,
    pub marks: Vec<(Range<usize>, Marks)>,
    pub links: Vec<(Range<usize>, String)>,
}

impl Inline {
    fn push(&mut self, s: &str, marks: Marks) {
        let s = if self.text.is_empty() {
            s.trim_start()
        } else if self.text.ends_with('\n') || (self.text.ends_with(' ') && !marks.code) {
            s.trim_start_matches(' ')
        } else {
            s
        };
        if s.is_empty() {
            return;
        }
        if s.starts_with('\n') {
            let keep = self.text.trim_end_matches(' ').len();
            let ends = self.marks.iter().map(|(r, _)| r.end).chain(self.links.iter().map(|(r, _)| r.end));
            if ends.max().is_none_or(|end| end <= keep) {
                self.text.truncate(keep);
            }
        }
        let start = self.text.len();
        self.text.push_str(s);
        let end = self.text.len();
        if marks == Marks::default() {
            return;
        }
        match self.marks.last_mut() {
            Some((r, m)) if *m == marks && r.end == start => r.end = end,
            _ => self.marks.push((start..end, marks)),
        }
    }

    fn append(&mut self, other: Inline) {
        let at = self.text.len();
        self.text.push_str(&other.text);
        self.marks.extend(other.marks.into_iter().map(|(r, m)| (r.start + at..r.end + at, m)));
        self.links.extend(other.links.into_iter().map(|(r, u)| (r.start + at..r.end + at, u)));
    }

    fn finish(mut self) -> Option<Inline> {
        let len = self.text.trim_end().len();
        self.text.truncate(len);
        let clamp = |r: &mut Range<usize>| *r = r.start.min(len)..r.end.min(len);
        self.marks.iter_mut().for_each(|(r, _)| clamp(r));
        self.links.iter_mut().for_each(|(r, _)| clamp(r));
        self.marks.retain(|(r, _)| !r.is_empty());
        self.links.retain(|(r, _)| !r.is_empty());
        (!self.text.is_empty()).then_some(self)
    }
}

#[derive(Debug)]
pub enum Block {
    Heading(u8, Inline),
    Paragraph(Inline),
    Code(String),
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Rule,
    /// Each row's cells on one line, the header first.
    Table(Vec<Inline>),
}

/// Where a link in `repo`'s README leads: as written when it is a web address, or within the
/// repository when it is relative, as GitHub reads it from `blob/HEAD/` (so `../../issues` is
/// the repository's issues). `None` for what should not be opened (`javascript:`, files).
pub fn resolve(link: &str, repo: &str) -> Option<String> {
    let link = link.trim().replace(' ', "%20");
    if link.is_empty() {
        return None;
    }
    let lower = link.to_ascii_lowercase();
    if ["https://", "http://", "mailto:"].iter().any(|s| lower.starts_with(s)) {
        return Some(link);
    }
    if let Some(rest) = link.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    if let Some(anchor) = link.strip_prefix('#') {
        return Some(format!("https://github.com/{repo}#{anchor}"));
    }
    let path = link.split(['/', '?', '#']).next().unwrap_or_default();
    if path.contains(':') {
        return None;
    }
    let split = link.find(['?', '#']).unwrap_or(link.len());
    let mut parts = vec!["blob", "HEAD"];
    for part in link[..split].split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let tail = if link[..split].ends_with('/') && parts.len() > 2 { "/" } else { "" };
    Some(format!("https://github.com/{repo}/{}{tail}{}", parts.join("/"), &link[split..]))
}

/// Text without GitHub's `:emoji:` codes, which only GitHub draws.
fn without_emoji_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(':') {
        let after = &rest[i + 1..];
        let code = after
            .find(':')
            .filter(|&j| j > 0 && after[..j].chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_+-".contains(c)));
        let before_ok = rest[..i].chars().next_back().is_none_or(char::is_whitespace);
        match code {
            Some(j) if before_ok && after[j + 1..].chars().next().is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation()) => {
                out.push_str(&rest[..i]);
                rest = &after[j + 1..];
                if out.is_empty() || out.ends_with(' ') {
                    rest = rest.trim_start_matches(' ');
                }
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Leaves out a first heading that only repeats one of `names`, the mod's name already on top
/// of its page.
pub fn drop_title(doc: &mut Vec<Block>, names: &[&str]) {
    let squash = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    if let Some(Block::Heading(_, h)) = doc.first()
        && names.iter().any(|n| squash(n) == squash(&h.text))
    {
        doc.remove(0);
    }
}

/// An image that only shows a project's state: build, version, downloads.
fn is_badge(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    ["shields.io", "badge", "travis-ci", "appveyor", "codecov"].iter().any(|b| u.contains(b))
}

/// The text of raw HTML: tags and comments dropped, the ones that end a line as line breaks.
fn html_text(html: &str) -> String {
    const BREAKS: [&str; 16] =
        ["br", "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "li", "tr", "ul", "ol", "table", "details", "summary"];
    let mut out = String::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |i| &after[i + 3..]);
        } else if let Some(after) = rest.strip_prefix('<') {
            let Some(end) = after.find('>') else { break };
            let name: String = after[..end]
                .trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if BREAKS.contains(&name.as_str()) {
                out.push('\n');
            } else {
                out.push(' ');
            }
            rest = &after[end + 1..];
        } else {
            let i = rest.find('<').unwrap_or(rest.len());
            out.push_str(&rest[..i]);
            rest = &rest[i..];
        }
    }
    let out = out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    let out = out.replace("&amp;", "&");
    let lines: Vec<String> = out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect();
    lines.join("\n")
}

enum FrameKind {
    Root,
    Quote,
    List(Option<u64>, Vec<Vec<Block>>),
    Item,
}

struct Frame {
    kind: FrameKind,
    blocks: Vec<Block>,
}

struct Builder<'a> {
    repo: &'a str,
    frames: Vec<Frame>,
    /// Containers past `MAX_DEPTH` not given a frame of their own.
    deep: usize,
    inline: Option<Inline>,
    heading: Option<u8>,
    strong: usize,
    italic: usize,
    strike: usize,
    /// Where the open link starts, and where it leads when it can be opened.
    link: Option<(usize, Option<String>)>,
    /// Inside an image: whether its text is shown.
    image: Option<bool>,
    code: Option<String>,
    html: Option<String>,
    cells: Vec<Inline>,
    rows: Option<Vec<Inline>>,
    /// Nothing but headings yet: images here are badges.
    top: bool,
}

impl Builder<'_> {
    fn marks(&self) -> Marks {
        Marks {
            strong: self.strong > 0,
            italic: self.italic > 0,
            code: false,
            strike: self.strike > 0,
            link: self.link.as_ref().is_some_and(|(_, u)| u.is_some()),
        }
    }

    fn text(&mut self, s: &str, marks: Marks) {
        self.inline.get_or_insert_default().push(s, marks);
    }

    fn add(&mut self, b: Block) {
        if !matches!(b, Block::Heading(..)) {
            self.top = false;
        }
        if let Some(f) = self.frames.last_mut() {
            f.blocks.push(b);
        }
    }

    /// Ends the open paragraph or heading.
    fn flush(&mut self) {
        let heading = self.heading.take();
        self.link = None;
        let Some(inline) = self.inline.take().and_then(Inline::finish) else { return };
        if self.rows.is_some() {
            self.cells.push(inline);
            return;
        }
        self.add(match heading {
            Some(level) => Block::Heading(level, inline),
            None => Block::Paragraph(inline),
        });
    }

    fn open(&mut self, kind: FrameKind) {
        self.flush();
        if self.frames.len() > MAX_DEPTH {
            self.deep += 1;
        } else {
            self.frames.push(Frame { kind, blocks: Vec::new() });
        }
    }

    fn close(&mut self) {
        self.flush();
        if self.deep > 0 {
            self.deep -= 1;
            return;
        }
        if self.frames.len() < 2 {
            return;
        }
        let Some(f) = self.frames.pop() else { return };
        let block = match f.kind {
            FrameKind::Root => return,
            FrameKind::Quote => Block::Quote(f.blocks),
            FrameKind::List(start, items) => Block::List { start, items },
            FrameKind::Item => {
                if let Some(Frame { kind: FrameKind::List(_, items), .. }) = self.frames.last_mut() {
                    items.push(f.blocks);
                } else {
                    f.blocks.into_iter().for_each(|b| self.add(b));
                }
                return;
            }
        };
        if !matches!(&block, Block::List { items, .. } if items.is_empty()) {
            self.add(block);
        }
    }

    fn event(&mut self, e: Event) {
        match e {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(s) => {
                if let Some(code) = &mut self.code {
                    code.push_str(&s);
                } else if let Some(show) = self.image {
                    if show {
                        let marks = self.marks();
                        self.text(&s, marks);
                    }
                } else {
                    let s = without_emoji_codes(&s);
                    if self.heading.is_none() && !s.trim().is_empty() {
                        self.top = false;
                    }
                    let marks = self.marks();
                    self.text(&s, marks);
                }
            }
            Event::Code(s) => {
                if self.image.is_none() {
                    let marks = Marks { code: true, ..self.marks() };
                    self.text(&s, marks);
                }
            }
            Event::Html(s) => match &mut self.html {
                Some(html) => html.push_str(&s),
                None => self.text(&html_text(&s), Marks::default()),
            },
            Event::InlineHtml(s) => {
                let tag = s.trim_start_matches(['<', '/']).to_ascii_lowercase();
                if tag.starts_with("br") {
                    self.text("\n", Marks::default());
                }
            }
            Event::SoftBreak => self.text(" ", Marks::default()),
            Event::HardBreak => self.text("\n", Marks::default()),
            Event::Rule => {
                self.flush();
                self.add(Block::Rule);
            }
            Event::TaskListMarker(done) => self.text(if done { "[x] " } else { "[ ] " }, Marks::default()),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => self.flush(),
            Tag::Heading { level, .. } => {
                self.flush();
                self.heading = Some(level as u8);
            }
            Tag::BlockQuote(_) => self.open(FrameKind::Quote),
            Tag::List(start) => self.open(FrameKind::List(start, Vec::new())),
            Tag::Item => self.open(FrameKind::Item),
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::HtmlBlock => {
                self.flush();
                self.html = Some(String::new());
            }
            Tag::Table(_) => {
                self.flush();
                self.rows = Some(Vec::new());
            }
            Tag::TableHead | Tag::TableRow => self.cells.clear(),
            Tag::TableCell => self.inline = None,
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.strong += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { dest_url, .. } => {
                let at = self.inline.as_ref().map_or(0, |i| i.text.len());
                self.link = Some((at, resolve(&dest_url, self.repo)));
            }
            Tag::Image { dest_url, .. } => {
                let linked = self.link.as_ref().is_some_and(|(_, u)| u.is_some());
                self.image = Some(linked && !self.top && !is_badge(&dest_url));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.flush(),
            TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item => self.close(),
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    let code = code.trim_end_matches(['\n', '\r']);
                    if !code.trim().is_empty() {
                        self.add(Block::Code(code.to_string()));
                    }
                }
            }
            TagEnd::HtmlBlock => {
                let text = html_text(&self.html.take().unwrap_or_default());
                if !text.is_empty() {
                    self.text(&text, Marks::default());
                }
                self.flush();
            }
            TagEnd::TableCell => self.flush(),
            TagEnd::TableHead | TagEnd::TableRow => {
                let mut row = Inline::default();
                for (i, cell) in std::mem::take(&mut self.cells).into_iter().enumerate() {
                    if i > 0 {
                        row.text.push_str("  ·  ");
                    }
                    row.append(cell);
                }
                if let (Some(rows), Some(row)) = (&mut self.rows, row.finish()) {
                    rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(rows) = self.rows.take().filter(|r| !r.is_empty()) {
                    self.add(Block::Table(rows));
                }
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.strong = self.strong.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => {
                if let (Some((start, Some(url))), Some(inline)) = (self.link.take(), &mut self.inline)
                    && start < inline.text.len()
                {
                    let end = inline.text.len();
                    inline.links.push((start..end, url));
                }
            }
            TagEnd::Image => self.image = None,
            _ => {}
        }
    }
}

/// `repo`'s README as blocks to draw; empty when it says nothing that can be shown.
pub fn parse(markdown: &str, repo: &str) -> Vec<Block> {
    let mut b = Builder {
        repo,
        frames: vec![Frame { kind: FrameKind::Root, blocks: Vec::new() }],
        deep: 0,
        inline: None,
        heading: None,
        strong: 0,
        italic: 0,
        strike: 0,
        link: None,
        image: None,
        code: None,
        html: None,
        cells: Vec::new(),
        rows: None,
        top: true,
    };
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for e in Parser::new_ext(markdown, options) {
        b.event(e);
    }
    b.flush();
    while b.frames.len() > 1 {
        b.deep = 0;
        b.close();
    }
    let mut doc = b.frames.pop().map(|f| f.blocks).unwrap_or_default();
    // A heading over nothing, as when its section was only images.
    let mut i = 0;
    while i < doc.len() {
        let empty = match (&doc[i], doc.get(i + 1)) {
            (Block::Heading(level, _), Some(Block::Heading(next, _))) => next <= level,
            (Block::Heading(..), None) => true,
            _ => false,
        };
        if empty {
            doc.remove(i);
            i = i.saturating_sub(1);
        } else {
            i += 1;
        }
    }
    doc
}

/// The blocks, in the page's type: body text in the second text colour, headings no bigger than
/// a section's title.
pub fn render(doc: &[Block], t: &Theme) -> Div {
    let mut n = 0;
    blocks(doc, t, &mut n, 12.)
}

fn blocks(list: &[Block], t: &Theme, n: &mut usize, gap: f32) -> Div {
    let mut d = div().flex().flex_col().gap(px(gap)).min_w(px(0.));
    for b in list {
        d = d.child(block(b, t, n));
    }
    d
}

fn block(b: &Block, t: &Theme, n: &mut usize) -> AnyElement {
    let body = || div().min_w(px(0.)).text_size(px(text::BODY.0)).line_height(px(text::BODY.1)).text_color(t.text2);
    match b {
        Block::Heading(level, s) => {
            let (size, line) = if *level <= 2 { text::TITLE } else { text::BODY };
            div()
                .min_w(px(0.))
                .pt(px(8.))
                .text_size(px(size))
                .line_height(px(line))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.text)
                .child(styled(s, t, n))
                .into_any_element()
        }
        Block::Paragraph(s) => body().child(styled(s, t, n)).into_any_element(),
        Block::Code(code) => div()
            .min_w(px(0.))
            .overflow_hidden()
            .px(px(12.))
            .py(px(10.))
            .rounded(px(crate::theme::radius::SMALL))
            .bg(t.control)
            .font_family(MONO_FONT)
            .text_size(px(text::CAPTION.0))
            .line_height(px(text::CAPTION.1 + 2.))
            .text_color(t.text2)
            .child(code.clone())
            .into_any_element(),
        Block::Quote(inner) => blocks(inner, t, n, 12.).pl(px(12.)).border_l_2().border_color(t.stroke_strong).into_any_element(),
        Block::List { start, items } => {
            let mut list = div().flex().flex_col().gap(px(6.)).min_w(px(0.));
            for (i, item) in items.iter().enumerate() {
                let marker = match start {
                    Some(first) => format!("{}.", first.saturating_add(i as u64)),
                    None => "•".to_string(),
                };
                list = list.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.))
                        .min_w(px(0.))
                        .child(body().flex_none().min_w(px(14.)).text_color(t.text3).child(marker))
                        .child(blocks(item, t, n, 6.).flex_1()),
                );
            }
            list.into_any_element()
        }
        Block::Rule => div().my(px(4.)).h(px(1.)).w_full().bg(t.stroke).into_any_element(),
        Block::Table(rows) => {
            let mut d = div().flex().flex_col().gap(px(4.)).min_w(px(0.));
            for (i, row) in rows.iter().enumerate() {
                let line = body().child(styled(row, t, n));
                d = d.child(if i == 0 { line.font_weight(FontWeight::MEDIUM).text_color(t.text) } else { line });
            }
            d.into_any_element()
        }
    }
}

fn highlight(m: Marks, t: &Theme) -> HighlightStyle {
    HighlightStyle {
        color: (m.strong || m.link).then_some(t.text),
        font_weight: m.strong.then_some(FontWeight::MEDIUM),
        font_style: m.italic.then_some(FontStyle::Italic),
        background_color: m.code.then_some(t.control),
        underline: m.link.then_some(UnderlineStyle { thickness: px(1.), color: Some(t.text3), wavy: false }),
        strikethrough: m.strike.then_some(StrikethroughStyle { thickness: px(1.), color: None }),
        ..Default::default()
    }
}

fn styled(s: &Inline, t: &Theme, n: &mut usize) -> AnyElement {
    let text = StyledText::new(s.text.clone())
        .with_highlights(s.marks.iter().map(|(r, m)| (r.clone(), highlight(*m, t))))
        .with_font_family_overrides(s.marks.iter().filter(|(_, m)| m.code).map(|(r, _)| (r.clone(), SharedString::from(MONO_FONT))));
    if s.links.is_empty() {
        return text.into_any_element();
    }
    *n += 1;
    let urls: Vec<String> = s.links.iter().map(|(_, u)| u.clone()).collect();
    InteractiveText::new(("readme-link", *n), text)
        .on_click(s.links.iter().map(|(r, _)| r.clone()).collect(), move |i, _, cx| {
            if let Some(url) = urls.get(i) {
                cx.open_url(url);
            }
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "owner/repo";

    #[test]
    fn links_resolve_against_the_repository() {
        let r = |l: &str| resolve(l, REPO);
        assert_eq!(r("https://example.com/a").as_deref(), Some("https://example.com/a"));
        assert_eq!(r("HTTP://example.com").as_deref(), Some("HTTP://example.com"));
        assert_eq!(r("mailto:a@b.c").as_deref(), Some("mailto:a@b.c"));
        assert_eq!(r("//cdn.example.com/x").as_deref(), Some("https://cdn.example.com/x"));
        assert_eq!(r("#install").as_deref(), Some("https://github.com/owner/repo#install"));
        assert_eq!(r("docs/USAGE.md").as_deref(), Some("https://github.com/owner/repo/blob/HEAD/docs/USAGE.md"));
        assert_eq!(r("./docs/a b.md#x").as_deref(), Some("https://github.com/owner/repo/blob/HEAD/docs/a%20b.md#x"));
        assert_eq!(r("/LICENSE").as_deref(), Some("https://github.com/owner/repo/blob/HEAD/LICENSE"));
        assert_eq!(r("/../../releases/latest").as_deref(), Some("https://github.com/owner/repo/releases/latest"));
        assert_eq!(r("../../../../../issues").as_deref(), Some("https://github.com/owner/repo/issues"));
        assert_eq!(r("docs/").as_deref(), Some("https://github.com/owner/repo/blob/HEAD/docs/"));
        assert_eq!(r("a/../b.md?raw=true").as_deref(), Some("https://github.com/owner/repo/blob/HEAD/b.md?raw=true"));
        assert_eq!(r("javascript:alert(1)"), None);
        assert_eq!(r("file:///C:/x"), None);
        assert_eq!(r("  "), None);
    }

    #[test]
    fn badges_at_the_top_are_left_out() {
        let doc = parse(
            "# Mod\n\n[![Build](https://img.shields.io/x.svg)](https://ci.example.com) ![Logo](logo.png)\n\nText with [![Video](shot.png)](https://youtu.be/x).",
            REPO,
        );
        assert_eq!(doc.len(), 2);
        let Block::Paragraph(p) = &doc[1] else { panic!("{doc:?}") };
        assert_eq!(p.text, "Text with Video.");
        assert_eq!(p.links, [(10..15, "https://youtu.be/x".to_string())]);
    }

    #[test]
    fn inline_marks_and_links() {
        let doc = parse("Some **bold** and *it* and `code` and [a link](docs/a.md).", REPO);
        let [Block::Paragraph(p)] = doc.as_slice() else { panic!("{doc:?}") };
        assert_eq!(p.text, "Some bold and it and code and a link.");
        assert_eq!(p.marks.len(), 4);
        assert!(p.marks[0].1.strong && p.marks[1].1.italic && p.marks[2].1.code && p.marks[3].1.link);
        assert_eq!(p.links[0].1, "https://github.com/owner/repo/blob/HEAD/docs/a.md");
    }

    #[test]
    fn emoji_codes_and_titles() {
        assert_eq!(without_emoji_codes(":scream: Where is it? I :+1: know. :man_shrugging:"), "Where is it? I know. ");
        assert_eq!(without_emoji_codes("at 10:30:00, see a:b:c and :Not:"), "at 10:30:00, see a:b:c and :Not:");
        let mut doc = parse("# Spore-Rich-Presence\n\nText", REPO);
        drop_title(&mut doc, &["Spore Rich Presence"]);
        assert!(matches!(doc.as_slice(), [Block::Paragraph(_)]));
        let mut doc = parse("# Installing\n\nText", REPO);
        drop_title(&mut doc, &["Spore Rich Presence"]);
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn html_keeps_its_text() {
        let doc =
            parse("<p align=\"center\">\n  <img src=\"x.png\">\n  <b>Hello</b> &amp; welcome<br>to it\n</p>\n<!-- hidden -->\n", REPO);
        let [Block::Paragraph(p)] = doc.as_slice() else { panic!("{doc:?}") };
        assert_eq!(p.text, "Hello & welcome\nto it");
        let doc = parse("Inline <kbd>Ctrl</kbd> key <br>\nnext", REPO);
        let [Block::Paragraph(p)] = doc.as_slice() else { panic!("{doc:?}") };
        assert_eq!(p.text, "Inline Ctrl key\nnext");
    }

    #[test]
    fn lists_nest_and_tables_become_rows() {
        let doc = parse("1. one\n2. two\n   - a\n     - deep\n   - b\n\n| A | B |\n|---|---|\n| 1 | **2** |\n", REPO);
        let [Block::List { start: Some(1), items }, Block::Table(rows)] = doc.as_slice() else { panic!("{doc:?}") };
        assert_eq!(items.len(), 2);
        assert!(matches!(items[1].as_slice(), [Block::Paragraph(_), Block::List { start: None, .. }]));
        assert_eq!(rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["A  ·  B", "1  ·  2"]);
        assert_eq!(rows[1].marks[0].0, 7..8);
    }

    #[test]
    fn odd_markdown_does_not_panic() {
        let t = Theme::new(gpui::WindowAppearance::Dark);
        let deep = format!("{}deep\n", "> ".repeat(100));
        let nested = (0..60).map(|i| format!("{}- level {i}\n", "  ".repeat(i))).collect::<String>();
        let samples = [
            "",
            "   \n\n  ",
            "<div><table><tr><td>a</td></tr></table></div>",
            "<img src=x>",
            "<!-- never closed",
            "<a href=\"x\">unclosed <b",
            "**bold *both** italic*",
            "[](empty) [x]() ![](y) [![](z)](w)",
            "```\n\n```\n```rust\nfn main() {}\n",
            "| a |\n|---|\n| é | ü |\n|",
            "- [ ] todo\n- [x] done",
            "Line  \nbreak\\\nagain",
            "# \n## ##\n###### six",
            "> - a\n>   > 1. b\n---\n***",
            "Unicode — ünïcödé 🎉 `日本` **語**",
            "[link](<a b>) <https://auto.link> www.example.com",
            "Footnote[^1]\n\n[^1]: note",
            deep.as_str(),
            nested.as_str(),
        ];
        for md in samples {
            let doc = parse(md, REPO);
            let _ = render(&doc, &t);
        }
    }
}
