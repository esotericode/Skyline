use crate::{
    formats::{HeaderRule, MAX_HEADER_BYTES, detect_header, inspect_format},
    model::{Digests, FileFormat, HeaderEvidence},
};
use anyhow::{Context, Result, bail};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            bail!("Job cancelled");
        }
        Ok(())
    }
}

struct Hashers {
    crc: crc32fast::Hasher,
    md5: Md5,
    sha1: Sha1,
    sha256: Sha256,
}

impl Hashers {
    fn new() -> Self {
        Self {
            crc: crc32fast::Hasher::new(),
            md5: Md5::new(),
            sha1: Sha1::new(),
            sha256: Sha256::new(),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        self.crc.update(bytes);
        self.md5.update(bytes);
        self.sha1.update(bytes);
        self.sha256.update(bytes);
    }

    fn finish(self) -> Digests {
        Digests {
            crc32: Some(format!("{:08x}", self.crc.finalize())),
            md5: Some(format!("{:x}", self.md5.finalize())),
            sha1: Some(format!("{:x}", self.sha1.finalize())),
            sha256: Some(format!("{:x}", self.sha256.finalize())),
        }
    }
}

pub fn hash_stream(
    reader: impl Read,
    cancel: &Cancellation,
    progress: impl FnMut(u64),
) -> Result<(u64, Digests)> {
    let hashed = hash_stream_with_header(reader, cancel, progress, |_| None)?;
    Ok((hashed.size, hashed.digests))
}

/// Whole-file digests, plus digests of the bytes after a recognized header.
pub struct Hashed {
    pub size: u64,
    pub digests: Digests,
    pub header: Option<HeaderEvidence>,
    pub format: Option<FileFormat>,
}

/// One pass over the bytes. `header` sees up to the first `MAX_HEADER_BYTES` bytes
/// and may name a header to exclude from a second, header-free set of digests. The
/// source is only read; nothing is stripped from the file itself.
pub fn hash_stream_with_header(
    mut reader: impl Read,
    cancel: &Cancellation,
    mut progress: impl FnMut(u64),
    header: impl FnOnce(&[u8]) -> Option<HeaderRule>,
) -> Result<Hashed> {
    let mut buffer = [0_u8; 64 * 1024];
    // One byte past the largest header shows whether any content follows it.
    let prefix = MAX_HEADER_BYTES + 1;
    let mut filled = 0;
    while filled < prefix {
        cancel.check()?;
        let count = reader
            .read(&mut buffer[filled..prefix])
            .context("Reading ROM bytes")?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    // A header must be followed by content; a file that is only a header has none.
    let rule = header(&buffer[..filled]).filter(|rule| rule.bytes < filled);
    let mut whole = Hashers::new();
    let mut content = rule.map(|_| Hashers::new());
    whole.update(&buffer[..filled]);
    if let (Some(rule), Some(content)) = (rule, content.as_mut()) {
        content.update(&buffer[rule.bytes..filled]);
    }
    let mut size = filled as u64;
    if filled > 0 {
        progress(size);
    }
    loop {
        cancel.check()?;
        let count = reader.read(&mut buffer).context("Reading ROM bytes")?;
        if count == 0 {
            break;
        }
        let bytes = &buffer[..count];
        whole.update(bytes);
        if let Some(content) = content.as_mut() {
            content.update(bytes);
        }
        size += count as u64;
        progress(size);
    }
    cancel.check()?;
    let header = rule.zip(content).map(|(rule, content)| HeaderEvidence {
        format: rule.format.into(),
        header_bytes: rule.bytes as u64,
        size: size - rule.bytes as u64,
        digests: content.finish(),
    });
    Ok(Hashed {
        size,
        digests: whole.finish(),
        header,
        format: None,
    })
}

/// Symlinks, and on Windows also junctions/mount points (name-surrogate reparse points).
/// Other Windows reparse points, such as OneDrive placeholders and deduplicated files,
/// hold their own data and are hashed like regular files.
pub fn is_link(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub fn hash_file(path: &Path, cancel: &Cancellation, progress: impl FnMut(u64)) -> Result<Hashed> {
    let before = std::fs::symlink_metadata(path).context("Inspecting source")?;
    if is_link(&before) || !before.is_file() {
        bail!("Only regular files are supported; links/reparse points are skipped");
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: inspect the link itself, not its target.
        options.custom_flags(0x0020_0000);
    }
    let file = options
        .open(path)
        .context("Opening source read-only without following a final link")?;
    let opened = file.metadata()?;
    if is_link(&opened) {
        bail!("Links/reparse points are skipped");
    }
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let mut format = None;
    let mut result = hash_stream_with_header(&file, cancel, progress, |prefix| {
        format = inspect_format(prefix, &extension, opened.len());
        detect_header(prefix, &extension, opened.len())
    })?;
    result.format = format;
    let after = file.metadata()?;
    let path_after = std::fs::symlink_metadata(path).context("Rechecking source path")?;
    if is_link(&path_after)
        || path_after.len() != after.len()
        || path_after.modified()? != after.modified()?
    {
        bail!("Source path changed while hashing; rescan when stable");
    }
    if opened.len() != result.size
        || opened.len() != after.len()
        || opened.modified()? != after.modified()?
    {
        bail!("Source changed while hashing; rescan when the file is stable");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != opened.dev()
            || before.ino() != opened.ino()
            || path_after.dev() != opened.dev()
            || path_after.ino() != opened.ino()
        {
            bail!("Source changed before hashing");
        }
    }
    Ok(result)
}
