//! Markdown -> RTF conversion (and the static "About" document), built on
//! pulldown-cmark. Pure logic with no Win32 dependencies.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

// Color table used by every document. The three entries map to:
//   cf1 = body text, cf2 = links/accent, cf3 = inline-code highlight background.
fn color_table(dark: bool) -> &'static str {
    if dark {
        r"{\colortbl;\red220\green220\blue220;\red88\green166\blue255;\red60\green60\blue60;}"
    } else {
        r"{\colortbl;\red24\green24\blue27;\red101\green117\blue133;\red246\green248\blue250;}"
    }
}

fn rtf_header(dark: bool) -> String {
    format!(
        r"{{\rtf1\ansi\deff0{{\fonttbl{{\f0 Segoe UI;}}{{\f1 Consolas;}}}}{}\paperw12240\paperh15840\margl720\margr720\viewkind4\uc1",
        color_table(dark)
    )
}

pub(crate) fn about_rtf(dark: bool) -> String {
    format!(
        r#"{}\pard\cf1\f0\fs40\b Markd\b0\par\pard\sa240\fs22 Lightweight native Markdown viewer and editor for Windows, built with Rust for speed, simplicity, and efficiency.\par\pard\sa140\b Author:\b0  {{\field{{\*\fldinst{{HYPERLINK "https://khalidutsob.com"}}}}{{\fldrslt{{\cf2\ul Khalid Utsob}}}}}}\ul0\cf1\par\pard\sa140\b GitHub:\b0  {{\field{{\*\fldinst{{HYPERLINK "https://github.com/en-arnob/markd"}}}}{{\fldrslt{{\cf2\ul en-arnob/markd}}}}}}\ul0\cf1\par\pard\sa140\b Version:\b0  1.0.3\par}}"#,
        rtf_header(dark)
    )
}

pub(crate) fn markdown_to_rtf(markdown: &str, dark: bool) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut out = rtf_header(dark);
    out.push_str(r"\pard\cf1\f0\fs22 ");
    let mut list_depth = 0usize;
    let mut in_code_block = false;
    let mut in_table_cell = false;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => out.push_str(r"\pard\sa180\cf1\f0\fs22 "),
                Tag::Heading { level, .. } => {
                    let size = heading_size(level);
                    out.push_str(&format!(r"\pard\sa220\b\fs{} ", size));
                }
                Tag::BlockQuote(_) => out.push_str(r"\pard\li360\sa180\i\cf2 "),
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                            format!("{}\\line ", escape_rtf(&lang))
                        }
                        _ => String::new(),
                    };
                    out.push_str(r"\pard\li240\ri240\sa200\cf1\f1\fs20 ");
                    if !language.is_empty() {
                        out.push_str(r"\b ");
                        out.push_str(&language);
                        out.push_str(r"\b0 ");
                    }
                }
                Tag::List(_) => {
                    list_depth += 1;
                }
                Tag::Item => {
                    let indent = list_depth.saturating_mul(360);
                    out.push_str(&format!(r"\pard\li{}\sa80 \bullet\tab ", indent));
                }
                Tag::Emphasis => out.push_str(r"\i "),
                Tag::Strong => out.push_str(r"\b "),
                Tag::Strikethrough => out.push_str(r"\strike "),
                Tag::Link { dest_url, .. } => {
                    out.push_str(r"\cf2\ul ");
                    if !dest_url.is_empty() {
                        out.push_str(&escape_rtf(&dest_url));
                        out.push_str(" - ");
                    }
                }
                Tag::Table(_) => out.push_str(r"\pard\sa160 "),
                Tag::TableHead => out.push_str(r"\b "),
                Tag::TableRow => {}
                Tag::TableCell => {
                    in_table_cell = true;
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => out.push_str(r"\par "),
                TagEnd::Heading(_) => out.push_str(r"\b0\fs22\par "),
                TagEnd::BlockQuote(_) => out.push_str(r"\i0\cf1\par "),
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    out.push_str(r"\par ");
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                    out.push_str(r"\par ");
                }
                TagEnd::Item => out.push_str(r"\par "),
                TagEnd::Emphasis => out.push_str(r"\i0 "),
                TagEnd::Strong => out.push_str(r"\b0 "),
                TagEnd::Strikethrough => out.push_str(r"\strike0 "),
                TagEnd::Link => out.push_str(r"\ul0\cf1 "),
                TagEnd::Table => out.push_str(r"\par "),
                TagEnd::TableHead => out.push_str(r"\b0\par "),
                TagEnd::TableRow => out.push_str(r"\par "),
                TagEnd::TableCell => {
                    in_table_cell = false;
                    out.push_str(r"\tab ");
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    out.push_str(&escape_rtf(&text).replace('\n', r"\line "));
                } else {
                    out.push_str(&escape_rtf(&text));
                }
                if in_table_cell {
                    out.push(' ');
                }
            }
            Event::Code(code) => {
                out.push_str(r"\f1\highlight3 ");
                out.push_str(&escape_rtf(&code));
                out.push_str(r"\highlight0\f0 ");
            }
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push_str(r"\line "),
            Event::Rule => {
                out.push_str(r"\pard\sa180 ________________________________________\par ")
            }
            Event::TaskListMarker(checked) => {
                out.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::Html(html) | Event::InlineHtml(html) => out.push_str(&escape_rtf(&html)),
            Event::FootnoteReference(note) => {
                out.push('[');
                out.push_str(&escape_rtf(&note));
                out.push(']');
            }
            _ => {}
        }
    }

    out.push('}');
    out
}

fn heading_size(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 40,
        HeadingLevel::H2 => 32,
        HeadingLevel::H3 => 28,
        HeadingLevel::H4 => 24,
        HeadingLevel::H5 => 22,
        HeadingLevel::H6 => 20,
    }
}

fn escape_rtf(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str(r"\\"),
            '{' => escaped.push_str(r"\{"),
            '}' => escaped.push_str(r"\}"),
            '\n' => escaped.push_str(r"\line "),
            '\r' => {}
            ch if ch.is_ascii() => escaped.push(ch),
            ch => escaped.push_str(&format!(r"\u{}?", ch as i32)),
        }
    }
    escaped
}
