//! An attached file turned into Markdown (`docs/SPEC.md` 5.17, ADR 0073): a PDF's text, a Word
//! file's paragraphs, headings, lists and tables, or a Markdown or plain text file as it is. It
//! does no I/O and says why it refused in a code the web screens put into words.

use std::io::{Cursor, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};

use catervas_core::folders::OWNER_EDIT_LIMIT_BYTES;
use quick_xml::Reader;
use quick_xml::events::Event;

/// The most one attached file may be, before it is converted: 10 MiB.
pub const ATTACH_MAX_BYTES: usize = 10 * 1024 * 1024;

/// The most a Word file's `word/document.xml` may hold once unzipped, so that a small zip cannot
/// fill the daemon's memory.
const WORD_XML_MAX_BYTES: u64 = 64 * 1024 * 1024;

const PDF_MEDIA_TYPE: &str = "application/pdf";
const WORD_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// Why a file was not turned into text. `code` is the word the web screens match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachRefusal {
    /// Not a PDF, a Word file, or UTF-8 text named `.md`, `.markdown` or `.txt`.
    UnsupportedType,
    /// A PDF or Word file with no text in it (a scan).
    NoText,
    /// A PDF or Word file that could not be read: damaged, encrypted, or past what it may unzip to.
    Unreadable,
    /// The file is over 10 MiB, or its text over the owner's edit limit.
    TooLarge,
}

impl AttachRefusal {
    /// The refusal's code on the wire (`snake_case`).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedType => "unsupported_type",
            Self::NoText => "no_text",
            Self::Unreadable => "unreadable",
            Self::TooLarge => "too_large",
        }
    }
}

/// A file's text and the media type it was read as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    /// The Markdown.
    pub text: String,
    /// `application/pdf`, the Word type, `text/markdown` or `text/plain`.
    pub media_type: &'static str,
}

/// Turns the file `name` holding `bytes` into Markdown. The size is checked first; what is
/// converted is decided by the bytes, a text file's by its name's ending.
///
/// # Errors
/// An [`AttachRefusal`] naming why the file cannot be used.
pub fn to_markdown(name: &str, bytes: &[u8]) -> Result<Converted, AttachRefusal> {
    if bytes.len() > ATTACH_MAX_BYTES {
        return Err(AttachRefusal::TooLarge);
    }
    let converted = if bytes.starts_with(b"%PDF-") {
        Converted {
            text: pdf_text(bytes)?,
            media_type: PDF_MEDIA_TYPE,
        }
    } else if let Some(text) = word_text(bytes)? {
        Converted {
            text,
            media_type: WORD_MEDIA_TYPE,
        }
    } else {
        plain_text(name, bytes)?
    };
    if converted.text.len() > OWNER_EDIT_LIMIT_BYTES {
        return Err(AttachRefusal::TooLarge);
    }
    Ok(converted)
}

/// A Markdown or plain text file, kept as it is.
fn plain_text(name: &str, bytes: &[u8]) -> Result<Converted, AttachRefusal> {
    let extension = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let media_type = match extension.as_deref() {
        Some("md" | "markdown") => "text/markdown",
        Some("txt") => "text/plain",
        _ => return Err(AttachRefusal::UnsupportedType),
    };
    let text = std::str::from_utf8(bytes).map_err(|_| AttachRefusal::UnsupportedType)?;
    Ok(Converted {
        text: text.to_owned(),
        media_type,
    })
}

/// A PDF's text, its pages separated by a blank line. `pdf-extract` can panic on a malformed
/// font, so a panic is an unreadable file, not a crashed daemon.
fn pdf_text(bytes: &[u8]) -> Result<String, AttachRefusal> {
    let extracted = catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(bytes)
    }))
    .map_err(|_| AttachRefusal::Unreadable)?
    .map_err(|_| AttachRefusal::Unreadable)?;
    // pdf-extract ends each page with a form feed.
    let pages: Vec<&str> = extracted
        .split('\u{c}')
        .map(str::trim)
        .filter(|page| !page.is_empty())
        .collect();
    if pages.is_empty() {
        return Err(AttachRefusal::NoText);
    }
    Ok(format!("{}\n", pages.join("\n\n")))
}

/// What one block of a Word file is, which decides how it is written and what joins it to the next.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Paragraph,
    Item,
    Row,
}

/// A Word file's text as Markdown, or `None` when the bytes are not a zip holding
/// `word/document.xml`.
fn word_text(bytes: &[u8]) -> Result<Option<String>, AttachRefusal> {
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return Ok(None);
    };
    let Ok(entry) = archive.by_name("word/document.xml") else {
        return Ok(None);
    };
    let mut xml = Vec::new();
    // One byte past the limit tells a file that is exactly at it from one that is over.
    entry
        .take(WORD_XML_MAX_BYTES + 1)
        .read_to_end(&mut xml)
        .map_err(|_| AttachRefusal::Unreadable)?;
    if xml.len() as u64 > WORD_XML_MAX_BYTES {
        return Err(AttachRefusal::Unreadable);
    }
    let blocks = word_blocks(&xml)?;
    if blocks.is_empty() {
        return Err(AttachRefusal::NoText);
    }
    let mut text = String::new();
    let mut previous: Option<Block> = None;
    for (kind, line) in &blocks {
        match previous {
            // List items and table rows sit on adjacent lines; everything else is set apart.
            Some(last) if last == *kind && *kind != Block::Paragraph => text.push('\n'),
            Some(_) => text.push_str("\n\n"),
            None => {}
        }
        text.push_str(line);
        previous = Some(*kind);
    }
    text.push('\n');
    Ok(Some(text))
}

/// The paragraph being read.
#[derive(Default)]
struct Para {
    heading: usize,
    list: bool,
    text: String,
}

/// Reads `word/document.xml` into its non-empty blocks, in order.
fn word_blocks(xml: &[u8]) -> Result<Vec<(Block, String)>, AttachRefusal> {
    let mut reader = Reader::from_reader(xml);
    let mut blocks: Vec<(Block, String)> = Vec::new();
    let mut para = Para::default();
    let mut in_text = false;
    // A table inside a table is flattened into its cell: only the outer table's rows count.
    let mut table_depth = 0_usize;
    let mut cell: Vec<String> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    loop {
        let event = reader.read_event().map_err(|_| AttachRefusal::Unreadable)?;
        match event {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_start = matches!(event, Event::Start(_));
                match e.local_name().as_ref() {
                    b"p" if is_start => para = Para::default(),
                    b"pStyle" => {
                        if let Ok(Some(attr)) = e.try_get_attribute("w:val") {
                            para.heading = heading_level(&attr.value);
                        }
                    }
                    b"numPr" => para.list = true,
                    b"t" if is_start => in_text = true,
                    b"tab" | b"br" => para.text.push(' '),
                    b"tbl" if is_start => table_depth += 1,
                    b"tr" if is_start && table_depth == 1 => row.clear(),
                    b"tc" if is_start && table_depth == 1 => cell.clear(),
                    _ => {}
                }
            }
            Event::End(ref e) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"p" => {
                    let text = para.text.trim().to_owned();
                    if table_depth > 0 {
                        if !text.is_empty() {
                            cell.push(text);
                        }
                    } else if !text.is_empty() {
                        blocks.push(paragraph_block(&para, text));
                    }
                    para = Para::default();
                }
                b"tc" if table_depth == 1 => row.push(cell.join(" ")),
                b"tr" if table_depth == 1 => {
                    if row.iter().any(|c| !c.is_empty()) {
                        blocks.push((Block::Row, row.join(" | ")));
                    }
                    row.clear();
                }
                b"tbl" => table_depth = table_depth.saturating_sub(1),
                _ => {}
            },
            Event::Text(ref t) if in_text => {
                para.text
                    .push_str(&t.decode().map_err(|_| AttachRefusal::Unreadable)?);
            }
            Event::CData(ref t) if in_text => {
                para.text
                    .push_str(&t.decode().map_err(|_| AttachRefusal::Unreadable)?);
            }
            Event::GeneralRef(ref r) if in_text => push_reference(&mut para.text, r)?,
            _ => {}
        }
    }
    Ok(blocks)
}

/// A paragraph's block: a heading, a list item, or plain text.
fn paragraph_block(para: &Para, text: String) -> (Block, String) {
    if para.heading > 0 {
        (
            Block::Paragraph,
            format!("{} {text}", "#".repeat(para.heading)),
        )
    } else if para.list {
        (Block::Item, format!("- {text}"))
    } else {
        (Block::Paragraph, text)
    }
}

/// The heading level a paragraph style names: `Title` is 1, `Heading1` to `Heading6` their number,
/// anything else none (0).
fn heading_level(style: &[u8]) -> usize {
    match style {
        b"Title" => 1,
        [
            b'H',
            b'e',
            b'a',
            b'd',
            b'i',
            b'n',
            b'g',
            digit @ b'1'..=b'6',
        ] => usize::from(digit - b'0'),
        _ => 0,
    }
}

/// Writes what an XML reference (`&amp;`, `&#233;`) stands for.
fn push_reference(
    out: &mut String,
    reference: &quick_xml::events::BytesRef<'_>,
) -> Result<(), AttachRefusal> {
    if let Some(c) = reference
        .resolve_char_ref()
        .map_err(|_| AttachRefusal::Unreadable)?
    {
        out.push(c);
        return Ok(());
    }
    match &**reference {
        b"amp" => out.push('&'),
        b"lt" => out.push('<'),
        b"gt" => out.push('>'),
        b"quot" => out.push('"'),
        b"apos" => out.push('\''),
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::{ATTACH_MAX_BYTES, AttachRefusal, to_markdown};

    const SPEC_PDF: &[u8] = include_bytes!("../tests/fixtures/attach/spec.pdf");
    const SCAN_PDF: &[u8] = include_bytes!("../tests/fixtures/attach/scan.pdf");
    const WORD: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

    /// A zip holding the given files, as a `.docx` is.
    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(body.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn paragraph(style: Option<&str>, list: bool, text: &str) -> String {
        let style = style.map_or(String::new(), |s| format!("<w:pStyle w:val=\"{s}\"/>"));
        let num = if list {
            "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>"
        } else {
            ""
        };
        format!("<w:p><w:pPr>{style}{num}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
    }

    #[test]
    fn turns_markdown_text_pdf_and_word_into_markdown() {
        let md = to_markdown("notes.md", b"# Hi\n\nthere\n").unwrap();
        assert_eq!(
            (md.text.as_str(), md.media_type),
            ("# Hi\n\nthere\n", "text/markdown")
        );
        let txt = to_markdown("notes.txt", b"plain").unwrap();
        assert_eq!((txt.text.as_str(), txt.media_type), ("plain", "text/plain"));

        let pdf = to_markdown("spec.pdf", SPEC_PDF).unwrap();
        assert!(pdf.text.contains("Product spec") && pdf.text.contains("It plans trips."));
        assert_eq!(pdf.media_type, "application/pdf");

        let table = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>\
            <w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc>\
            <w:tc><w:p><w:r><w:t>d</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
        let body = format!(
            "<?xml version=\"1.0\"?><w:document xmlns:w=\"w\"><w:body>{}{}{}{table}</w:body></w:document>",
            paragraph(Some("Title"), false, "Trips"),
            paragraph(Some("Heading2"), false, "Goals"),
            paragraph(None, true, "Plan a trip"),
        );
        let docx = to_markdown("trips.docx", &zip_of(&[("word/document.xml", &body)])).unwrap();
        assert_eq!(
            docx.text,
            "# Trips\n\n## Goals\n\n- Plan a trip\n\na | b\nc | d\n"
        );
        assert_eq!(docx.media_type, WORD);
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        let refused = |name: &str, bytes: &[u8]| to_markdown(name, bytes).unwrap_err();
        assert_eq!(refused("scan.pdf", SCAN_PDF), AttachRefusal::NoText);
        assert_eq!(
            refused("photo.png", b"\x89PNG\r\n\x1a\n\0\0"),
            AttachRefusal::UnsupportedType
        );
        assert_eq!(
            refused("notes.md", b"\xff\xfe\xfa"),
            AttachRefusal::UnsupportedType
        );
        let no_word = zip_of(&[("other.xml", "<a/>")]);
        assert_eq!(refused("x.docx", &no_word), AttachRefusal::UnsupportedType);
        assert_eq!(
            refused("bad.pdf", b"%PDF-1.4 garbage"),
            AttachRefusal::Unreadable
        );
        assert_eq!(
            refused("big.md", &vec![b'a'; ATTACH_MAX_BYTES + 1]),
            AttachRefusal::TooLarge
        );
        let big_text = vec![b'a'; 262_145];
        assert_eq!(refused("long.md", &big_text), AttachRefusal::TooLarge);
        let codes = [
            (AttachRefusal::UnsupportedType, "unsupported_type"),
            (AttachRefusal::NoText, "no_text"),
            (AttachRefusal::Unreadable, "unreadable"),
            (AttachRefusal::TooLarge, "too_large"),
        ];
        for (refusal, code) in codes {
            assert_eq!(refusal.code(), code);
        }
    }
}
