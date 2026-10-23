//! Zip files of a claude.ai export for tests, in this crate and in others. None holds a real
//! conversation: their content comes from `catervas_core::claude_export::fixtures`.

use std::io::{Cursor, Write};

use catervas_core::claude_export::fixtures::EXPORT_2026_10;
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

/// A header field to change in both the local and the central header of a stored entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipPatch {
    /// The declared uncompressed size, which a header can lie about.
    DeclaredSize(u32),
    /// The "encrypted" flag.
    Encrypted,
    /// The compression method.
    Method(u16),
    /// The checksum, to one the content does not have.
    BadChecksum,
}

/// What the zip writer is given in place of a name it would tidy (a leading `/`, a `..`): the same
/// length, so the real name goes in afterwards.
fn placeholder(name: &str) -> String {
    name.replace('/', "S").replace('.', "D")
}

/// A zip of `entries`, each deflated, named exactly as given even when the name is one a writer
/// would tidy (`../x`, `/x`), as a hostile zip's would be.
///
/// # Panics
///
/// When the zip cannot be written to memory, which it can always be.
#[must_use]
pub fn an_export_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, contents) in entries {
        writer
            .start_file(placeholder(name), options)
            .expect("an entry");
        writer.write_all(contents).expect("its bytes");
    }
    let mut bytes = writer.finish().expect("a zip").into_inner();
    for (name, _) in entries {
        let fake = placeholder(name);
        if fake != *name {
            let mut at = 0;
            while let Some(found) = bytes[at..]
                .windows(fake.len())
                .position(|window| window == fake.as_bytes())
            {
                bytes[at + found..at + found + name.len()].copy_from_slice(name.as_bytes());
                at += found + name.len();
            }
        }
    }
    bytes
}

/// The hand-written export of `catervas_core::claude_export::fixtures`, its files at the root.
#[must_use]
pub fn the_2026_10_export() -> Vec<u8> {
    let entries: Vec<(&str, &[u8])> = EXPORT_2026_10
        .iter()
        .map(|(name, text)| (*name, text.as_bytes()))
        .collect();
    an_export_zip(&entries)
}

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A zip of one stored entry whose local and central headers both carry `patch`.
///
/// # Panics
///
/// When the zip cannot be written to memory, which it can always be.
#[must_use]
pub fn a_patched_zip(name: &str, contents: &[u8], patch: ZipPatch) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file(name, options).expect("an entry");
    writer.write_all(contents).expect("its bytes");
    let mut bytes = writer.finish().expect("a zip").into_inner();
    let central = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x01\x02")
        .expect("a central header");
    // Offsets from each header's signature: flags, method, checksum, uncompressed size.
    for (start, flags, method, checksum, size) in [(0, 6, 8, 14, 22), (central, 8, 10, 16, 24)] {
        match patch {
            ZipPatch::DeclaredSize(declared) => put32(&mut bytes, start + size, declared),
            ZipPatch::Encrypted => {
                let old = u16::from_le_bytes([bytes[start + flags], bytes[start + flags + 1]]);
                put16(&mut bytes, start + flags, old | 1);
            }
            ZipPatch::Method(method_id) => put16(&mut bytes, start + method, method_id),
            ZipPatch::BadChecksum => {
                let old = u32::from_le_bytes([
                    bytes[start + checksum],
                    bytes[start + checksum + 1],
                    bytes[start + checksum + 2],
                    bytes[start + checksum + 3],
                ]);
                put32(&mut bytes, start + checksum, old ^ 0xFFFF_FFFF);
            }
        }
    }
    bytes
}

/// A zip with one symbolic link, named `name`, pointing at `target`.
///
/// # Panics
///
/// When the zip cannot be written to memory, which it can always be.
#[must_use]
pub fn a_zip_with_a_symlink(name: &str, target: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink(name, target, SimpleFileOptions::default())
        .expect("a link");
    writer.finish().expect("a zip").into_inner()
}
