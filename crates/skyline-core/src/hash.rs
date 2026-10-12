use crate::model::Digests;
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

pub fn hash_stream(
    mut reader: impl Read,
    cancel: &Cancellation,
    mut progress: impl FnMut(u64),
) -> Result<(u64, Digests)> {
    let mut crc = crc32fast::Hasher::new();
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0;
    loop {
        cancel.check()?;
        let count = reader.read(&mut buffer).context("Reading ROM bytes")?;
        if count == 0 {
            break;
        }
        let bytes = &buffer[..count];
        crc.update(bytes);
        md5.update(bytes);
        sha1.update(bytes);
        sha256.update(bytes);
        size += count as u64;
        progress(size);
    }
    cancel.check()?;
    Ok((
        size,
        Digests {
            crc32: Some(format!("{:08x}", crc.finalize())),
            md5: Some(format!("{:x}", md5.finalize())),
            sha1: Some(format!("{:x}", sha1.finalize())),
            sha256: Some(format!("{:x}", sha256.finalize())),
        },
    ))
}

pub fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub fn hash_file(
    path: &Path,
    cancel: &Cancellation,
    progress: impl FnMut(u64),
) -> Result<(u64, Digests)> {
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
    let result = hash_stream(&file, cancel, progress)?;
    let after = file.metadata()?;
    let path_after = std::fs::symlink_metadata(path).context("Rechecking source path")?;
    if is_link(&path_after)
        || path_after.len() != after.len()
        || path_after.modified()? != after.modified()?
    {
        bail!("Source path changed while hashing; rescan when stable");
    }
    if opened.len() != result.0
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
