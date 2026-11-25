//! Minimal, dependency-free markdown -> PDF renderer for the guides.
//!
//! The output is deterministic (no dates, no ids), so `pla-build --check` can compare it byte
//! for byte. It supports `#` headings, paragraphs, `-`/`*` and `1.` lists, and fenced code
//! blocks; inline `code`, **bold** and [text](url) are flattened to plain text. It uses the
//! PDF base-14 fonts (Helvetica, Helvetica-Bold, Courier) with `WinAnsi` encoding, so nothing
//! is embedded and the file stays plain ASCII.

use crate::verified::heading_hashes;
use std::fmt::Write as _;

const PAGE_W: f64 = 595.0; // A4, in points
const PAGE_H: f64 = 842.0;
const MARGIN: f64 = 56.0;
const TEXT_W: f64 = PAGE_W - 2.0 * MARGIN;

#[derive(Debug, PartialEq)]
enum Block {
    Heading(u64, String),
    Para(String),
    Item { marker: String, text: String },
    Code(Vec<String>),
}

#[derive(Clone, Copy, PartialEq)]
enum Font {
    Regular,
    Bold,
    Mono,
}

impl Font {
    fn name(self) -> &'static str {
        match self {
            Font::Regular => "F1",
            Font::Bold => "F2",
            Font::Mono => "F3",
        }
    }
}

// ---------- markdown ----------

fn parse(md: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut code: Option<Vec<String>> = None;
    let flush = |para: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            blocks.push(Block::Para(para.join(" ")));
            para.clear();
        }
    };
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            flush(&mut para, &mut blocks);
            match code.take() {
                Some(lines) => blocks.push(Block::Code(lines)),
                None => code = Some(Vec::new()),
            }
            continue;
        }
        if let Some(lines) = code.as_mut() {
            lines.push(line.to_string());
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut blocks);
            continue;
        }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && t[hashes..].starts_with(' ') {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Heading(
                hashes as u64,
                t[hashes..].trim().to_string(),
            ));
            continue;
        }
        if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Item {
                marker: "\u{2022}".into(),
                text: rest.to_string(),
            });
            continue;
        }
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && t[digits..].starts_with(". ") {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Item {
                marker: t[..=digits].to_string(),
                text: t[digits + 2..].to_string(),
            });
            continue;
        }
        // An indented line right after a list item continues it.
        if line.starts_with("  ") && para.is_empty() {
            if let Some(Block::Item { text, .. }) = blocks.last_mut() {
                text.push(' ');
                text.push_str(t);
                continue;
            }
        }
        para.push(t.to_string());
    }
    flush(&mut para, &mut blocks);
    if let Some(lines) = code {
        blocks.push(Block::Code(lines));
    }
    blocks
}

/// Flattens inline markdown: `[text](url)` -> `text (url)`, drops `**` and backticks.
fn inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        match (after.find("]("), after.find(')')) {
            (Some(mid), Some(close)) if mid < close => {
                out.push_str(&rest[..open]);
                let _ = write!(out, "{} ({})", &after[..mid], &after[mid + 2..close]);
                rest = &after[close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "")
}

// ---------- layout ----------

/// Approximate advance width of `c` in a 1-point font (Courier is exactly 0.6).
fn char_width(c: char, font: Font) -> f64 {
    if font == Font::Mono {
        return 0.6;
    }
    let w = match c {
        'i' | 'j' | 'l' | '\'' | '|' | '.' | ',' | ':' | ';' | '!' => 0.24,
        ' ' | 'f' | 't' | 'r' | 'I' | '(' | ')' | '[' | ']' | '/' | '-' => 0.33,
        'm' | 'w' | 'M' | 'W' => 0.86,
        'A'..='Z' => 0.68,
        '0'..='9' => 0.556,
        _ => 0.53,
    };
    if font == Font::Bold {
        w * 1.06
    } else {
        w
    }
}

fn width(s: &str, size: f64, font: Font) -> f64 {
    s.chars().map(|c| char_width(c, font)).sum::<f64>() * size
}

/// Splits `text` into lines no wider than `max` points.
fn wrap(text: &str, max: f64, size: f64, font: Font) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let candidate = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if width(&candidate, size, font) <= max || cur.is_empty() && width(word, size, font) <= max
        {
            cur = candidate;
            continue;
        }
        if !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        // A word longer than a whole line is cut by characters.
        let mut piece = String::new();
        for c in word.chars() {
            if width(&format!("{piece}{c}"), size, font) > max && !piece.is_empty() {
                lines.push(std::mem::take(&mut piece));
            }
            piece.push(c);
        }
        cur = piece;
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Escapes text for a PDF string in `WinAnsi` encoding (non-Latin characters become `?`).
fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            '\u{2022}' => out.push_str("\\225"),
            '\u{2013}' => out.push_str("\\226"),
            '\u{2014}' => out.push_str("\\227"),
            '\u{2018}' => out.push_str("\\221"),
            '\u{2019}' => out.push_str("\\222"),
            '\u{201C}' => out.push_str("\\223"),
            '\u{201D}' => out.push_str("\\224"),
            '\u{2026}' => out.push_str("\\205"),
            '\u{2192}' => out.push_str("->"),
            '\u{A0}'..='\u{FF}' => {
                let _ = write!(out, "\\{:03o}", c as u32);
            }
            _ => out.push('?'),
        }
    }
    out
}

struct Pages {
    done: Vec<String>,
    cur: String,
    y: f64,
}

impl Pages {
    fn new() -> Self {
        Pages {
            done: Vec::new(),
            cur: String::new(),
            y: PAGE_H - MARGIN,
        }
    }

    fn break_page(&mut self) {
        self.done.push(std::mem::take(&mut self.cur));
        self.y = PAGE_H - MARGIN;
    }

    fn gap(&mut self, h: f64) {
        self.y -= h;
    }

    fn text(&mut self, font: Font, size: f64, x: f64, s: &str) {
        let lead = size * 1.4;
        if self.y - lead < MARGIN {
            self.break_page();
        }
        self.y -= lead;
        let _ = writeln!(
            self.cur,
            "BT /{} {size:.1} Tf {x:.1} {:.1} Td ({}) Tj ET",
            font.name(),
            self.y,
            escape(s)
        );
    }
}

/// Renders markdown to a complete PDF file.
#[must_use]
pub fn render(md: &str) -> String {
    let mut p = Pages::new();
    for (i, block) in parse(md).iter().enumerate() {
        match block {
            Block::Heading(level, text) => {
                let size = match heading_hashes(*level) {
                    1 => 20.0,
                    2 => 15.0,
                    _ => 12.5,
                };
                if i > 0 {
                    p.gap(size * 0.6);
                }
                for line in wrap(&inline(text), TEXT_W, size, Font::Bold) {
                    p.text(Font::Bold, size, MARGIN, &line);
                }
                p.gap(4.0);
            }
            Block::Para(text) => {
                for line in wrap(&inline(text), TEXT_W, 11.0, Font::Regular) {
                    p.text(Font::Regular, 11.0, MARGIN, &line);
                }
                p.gap(6.0);
            }
            Block::Item { marker, text } => {
                let lines = wrap(&inline(text), TEXT_W - 22.0, 11.0, Font::Regular);
                for (n, line) in lines.iter().enumerate() {
                    if n == 0 {
                        // Marker and first line share a baseline.
                        p.text(Font::Regular, 11.0, MARGIN + 22.0, line);
                        let y = p.y;
                        let _ = writeln!(
                            p.cur,
                            "BT /F1 11.0 Tf {:.1} {y:.1} Td ({}) Tj ET",
                            MARGIN + 6.0,
                            escape(marker)
                        );
                    } else {
                        p.text(Font::Regular, 11.0, MARGIN + 22.0, line);
                    }
                }
                p.gap(3.0);
            }
            Block::Code(lines) => {
                p.gap(2.0);
                let max_chars = ((TEXT_W - 12.0) / (0.6 * 9.0)) as usize;
                for line in lines {
                    let chars: Vec<char> = line.chars().collect();
                    if chars.is_empty() {
                        p.text(Font::Mono, 9.0, MARGIN + 12.0, "");
                    }
                    for chunk in chars.chunks(max_chars.max(1)) {
                        let s: String = chunk.iter().collect();
                        p.text(Font::Mono, 9.0, MARGIN + 12.0, &s);
                    }
                }
                p.gap(8.0);
            }
        }
    }
    p.break_page();
    let total = p.done.len();
    let pages: Vec<String> = p
        .done
        .iter()
        .enumerate()
        .map(|(n, body)| {
            let footer = format!("{} / {total}", n + 1);
            let x = PAGE_W / 2.0 - width(&footer, 9.0, Font::Regular) / 2.0;
            format!(
                "{body}BT /F1 9.0 Tf {x:.1} {:.1} Td ({footer}) Tj ET\n",
                MARGIN / 2.0
            )
        })
        .collect();
    write_pdf(&pages)
}

// ---------- file ----------

/// Assembles the PDF objects, the cross-reference table and the trailer.
fn write_pdf(pages: &[String]) -> String {
    let n_pages = pages.len();
    let kids: Vec<String> = (0..n_pages).map(|i| format!("{} 0 R", 6 + 2 * i)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n_pages} >>",
            kids.join(" ")
        ),
    ];
    for base in ["Helvetica", "Helvetica-Bold", "Courier"] {
        objects.push(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>"
        ));
    }
    for (i, content) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
             /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R >> >> /Contents {} 0 R >>",
            7 + 2 * i
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for off in offsets {
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_supported_markdown() {
        let md =
            "# Title\n\nSome text\ncontinued.\n\n- one\n  more\n2. two\n\n```sh\nverus x.rs\n```\n";
        assert_eq!(
            parse(md),
            vec![
                Block::Heading(1, "Title".into()),
                Block::Para("Some text continued.".into()),
                Block::Item {
                    marker: "\u{2022}".into(),
                    text: "one more".into()
                },
                Block::Item {
                    marker: "2.".into(),
                    text: "two".into()
                },
                Block::Code(vec!["verus x.rs".into()]),
            ]
        );
    }

    #[test]
    fn inline_markup_is_flattened() {
        assert_eq!(
            inline("Run `verus` and read **the** [guide](https://x.org)."),
            "Run verus and read the guide (https://x.org)."
        );
    }

    #[test]
    fn text_is_escaped_for_pdf_strings() {
        assert_eq!(
            escape("a (b) \\ é \u{2022} \u{2192} \u{4e2d}"),
            "a \\(b\\) \\\\ \\351 \\225 -> ?"
        );
    }

    #[test]
    fn wrapped_lines_fit_the_width() {
        let text = "word ".repeat(200);
        for line in wrap(&text, 200.0, 11.0, Font::Regular) {
            assert!(width(&line, 11.0, Font::Regular) <= 200.0, "{line}");
        }
    }

    #[test]
    fn output_is_deterministic_paginated_and_has_a_valid_xref() {
        let md = format!("# Guide\n\n{}", "A paragraph of text.\n\n".repeat(120));
        let pdf = render(&md);
        assert_eq!(pdf, render(&md), "same input, same bytes");
        assert!(pdf.is_ascii() && pdf.starts_with("%PDF-1.4\n") && pdf.ends_with("%%EOF\n"));
        let count: usize = pdf
            .split("/Count ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(count > 1, "long text spans several pages");
        // Every xref offset points at the start of its object.
        let xref_at: usize = pdf
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let table: Vec<&str> = pdf[xref_at..]
            .lines()
            .skip(3)
            .take_while(|l| l.ends_with(" n "))
            .collect();
        for (i, row) in table.iter().enumerate() {
            let off: usize = row[..10].parse().unwrap();
            assert!(
                pdf[off..].starts_with(&format!("{} 0 obj", i + 1)),
                "object {}",
                i + 1
            );
        }
    }
}
