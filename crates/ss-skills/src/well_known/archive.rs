//! Unpack a well-known archive without following links or leaving the skill directory.

use std::io::{self, Read, Write};
use std::path::Path;

use anyhow::{bail, Result};
use flate2::read::{DeflateDecoder, GzDecoder};
use tar::Archive;

pub(crate) const MAX_UNPACKED_BYTES: u64 = 50 * 1024 * 1024;
pub(crate) const MAX_FILES: usize = 1000;

pub(crate) fn unpack_archive(bytes: &[u8], artifact_url: &str, dest: &Path) -> Result<()> {
    if is_zip(bytes, artifact_url) {
        return unpack_zip(bytes, dest);
    }
    if is_tar_gz(bytes, artifact_url) {
        return unpack_tar_gz(bytes, dest);
    }
    bail!("Unsupported well-known archive format");
}

fn is_zip(bytes: &[u8], artifact_url: &str) -> bool {
    artifact_url.to_ascii_lowercase().ends_with(".zip") || bytes.starts_with(&[0x50, 0x4b])
}

fn is_tar_gz(bytes: &[u8], artifact_url: &str) -> bool {
    let lower = artifact_url.to_ascii_lowercase();
    lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || bytes.starts_with(&[0x1f, 0x8b])
}

pub(crate) fn safe_archive_path(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.contains('\0') || raw.starts_with('/') || raw.starts_with('\\') {
        return None;
    }
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return None;
    }
    if raw.contains('\\') {
        return None;
    }
    let parts: Vec<&str> = raw.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() || parts.iter().any(|part| *part == "." || *part == "..") {
        return None;
    }
    Some(parts.join("/"))
}

struct Budget {
    files: usize,
    bytes: u64,
}

impl Budget {
    fn take(&mut self, size: u64) -> Result<()> {
        if self.files >= MAX_FILES {
            bail!("Archive contains too many files");
        }
        self.bytes = self.bytes.saturating_add(size);
        if self.bytes > MAX_UNPACKED_BYTES {
            bail!("Archive exceeds maximum unpacked size");
        }
        self.files += 1;
        Ok(())
    }
}

fn write_file(dest: &Path, rel: &str, bytes: &[u8], budget: &mut Budget) -> Result<()> {
    let rel = safe_archive_path(rel).ok_or_else(|| anyhow::anyhow!("Unsafe archive path"))?;
    budget.take(bytes.len() as u64)?;
    let path = dest.join(&rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| anyhow::anyhow!("Archive contains a duplicate path"))?;
    file.write_all(bytes)?;
    Ok(())
}

fn unpack_tar_gz(bytes: &[u8], dest: &Path) -> Result<()> {
    let mut archive = Archive::new(GzDecoder::new(bytes));
    archive.set_preserve_permissions(false);
    archive.set_unpack_xattrs(false);
    let mut budget = Budget { files: 0, bytes: 0 };
    for entry in archive.entries()? {
        let entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            bail!("Archive links are not supported");
        }
        if !kind.is_file() {
            continue;
        }
        let size = entry.size();
        if size > MAX_UNPACKED_BYTES {
            bail!("Archive exceeds maximum unpacked size");
        }
        let path = entry
            .path()
            .map_err(|_| anyhow::anyhow!("Unsafe archive path"))?
            .to_string_lossy()
            .into_owned();
        let mut content = Vec::new();
        entry.take(size).read_to_end(&mut content)?;
        if content.len() as u64 != size {
            bail!("Archive entry ended early");
        }
        write_file(dest, &path, &content, &mut budget)?;
    }
    Ok(())
}

fn unpack_zip(bytes: &[u8], dest: &Path) -> Result<()> {
    let eocd = find_eocd(bytes).ok_or_else(|| anyhow::anyhow!("Invalid zip archive"))?;
    let count = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]) as usize;
    let offset = u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    if count == 0xffff || offset == 0xffff_ffff {
        bail!("Zip64 archives are not supported");
    }
    if count > MAX_FILES {
        bail!("Archive contains too many files");
    }
    let mut cursor = offset;
    let mut budget = Budget { files: 0, bytes: 0 };
    for _ in 0..count {
        if cursor + 46 > bytes.len() || bytes[cursor..cursor + 4] != [0x50, 0x4b, 0x01, 0x02] {
            bail!("Invalid zip central directory");
        }
        let made_by = u16::from_le_bytes([bytes[cursor + 4], bytes[cursor + 5]]);
        let flags = u16::from_le_bytes([bytes[cursor + 8], bytes[cursor + 9]]);
        let method = u16::from_le_bytes([bytes[cursor + 10], bytes[cursor + 11]]);
        let compressed =
            u32::from_le_bytes(bytes[cursor + 20..cursor + 24].try_into().unwrap()) as usize;
        let uncompressed =
            u32::from_le_bytes(bytes[cursor + 24..cursor + 28].try_into().unwrap()) as u64;
        let name_len = u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]) as usize;
        let external = u32::from_le_bytes(bytes[cursor + 38..cursor + 42].try_into().unwrap());
        let local_offset =
            u32::from_le_bytes(bytes[cursor + 42..cursor + 46].try_into().unwrap()) as usize;
        let name_start = cursor + 46;
        let name_end = name_start + name_len;
        if name_end > bytes.len() {
            bail!("Invalid zip entry name");
        }
        let name = std::str::from_utf8(&bytes[name_start..name_end])
            .map_err(|_| anyhow::anyhow!("Unsafe archive path"))?;
        cursor = name_end + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        if made_by >> 8 == 3 && (external >> 16) & 0o170000 == 0o120000 {
            bail!("Archive links are not supported");
        }
        if flags & 1 == 1 {
            bail!("Encrypted zip entries are not supported");
        }
        if method != 0 && method != 8 {
            bail!("Unsupported zip compression");
        }
        if uncompressed > MAX_UNPACKED_BYTES {
            bail!("Archive exceeds maximum unpacked size");
        }
        let data = zip_entry_bytes(bytes, local_offset, compressed)?;
        let plain = if method == 0 {
            data
        } else {
            inflate(&data, uncompressed)?
        };
        if plain.len() as u64 != uncompressed {
            bail!("Zip entry size does not match the central directory");
        }
        write_file(dest, name, &plain, &mut budget)?;
    }
    Ok(())
}

fn zip_entry_bytes(bytes: &[u8], local_offset: usize, compressed: usize) -> Result<Vec<u8>> {
    if local_offset + 30 > bytes.len()
        || bytes[local_offset..local_offset + 4] != [0x50, 0x4b, 0x03, 0x04]
    {
        bail!("Invalid zip local header");
    }
    let name_len =
        u16::from_le_bytes([bytes[local_offset + 26], bytes[local_offset + 27]]) as usize;
    let extra_len =
        u16::from_le_bytes([bytes[local_offset + 28], bytes[local_offset + 29]]) as usize;
    let start = local_offset + 30 + name_len + extra_len;
    let end = start + compressed;
    if end > bytes.len() {
        bail!("Zip entry extends past the archive");
    }
    Ok(bytes[start..end].to_vec())
}

fn inflate(data: &[u8], expected: u64) -> Result<Vec<u8>> {
    let mut decoder = DeflateDecoder::new(data);
    let mut out = Vec::new();
    let mut limited = Limited::new(&mut out, expected);
    io::copy(&mut decoder, &mut limited)?;
    Ok(out)
}

struct Limited<'a> {
    inner: &'a mut Vec<u8>,
    remaining: u64,
}

impl<'a> Limited<'a> {
    fn new(inner: &'a mut Vec<u8>, remaining: u64) -> Self {
        Self { inner, remaining }
    }
}

impl Write for Limited<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() as u64 > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Archive exceeds maximum unpacked size",
            ));
        }
        self.inner.extend_from_slice(buf);
        self.remaining -= buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    let start = bytes.len().saturating_sub(22 + 65_535);
    let mut index = bytes.len() - 22;
    loop {
        if bytes[index..index + 4] == [0x50, 0x4b, 0x05, 0x06] {
            let comment_len = u16::from_le_bytes([bytes[index + 20], bytes[index + 21]]) as usize;
            if index + 22 + comment_len == bytes.len() {
                return Some(index);
            }
        }
        if index == start {
            return None;
        }
        index -= 1;
    }
}
