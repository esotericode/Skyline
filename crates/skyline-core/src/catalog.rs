use crate::{hash::Cancellation, model::*};
use anyhow::{Context, Result, bail};
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_DAT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 1_000_000;
const MAX_TEXT: usize = 8192;

pub struct ImportedCatalog {
    pub catalog: Catalog,
    pub snapshot: Vec<u8>,
}

pub fn import_path(path: &Path, system: &str, cancel: &Cancellation) -> Result<ImportedCatalog> {
    let mut file = std::fs::File::open(path)
        .context("Opening local DAT")?
        .take(MAX_DAT_BYTES + 1);
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        cancel.check()?;
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() as u64 > MAX_DAT_BYTES {
            bail!("DAT exceeds the 64 MiB import limit");
        }
    }
    let origin = path.canonicalize().context("Resolving local DAT origin")?;
    let catalog = parse_dat(&bytes, &origin, system, cancel)?;
    Ok(ImportedCatalog {
        catalog,
        snapshot: bytes,
    })
}

fn attributes(event: &BytesStart<'_>, reader: &Reader<&[u8]>) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for attribute in event.attributes() {
        let attribute = attribute.context("Invalid/duplicate XML attribute")?;
        let key = std::str::from_utf8(attribute.key.as_ref())?.to_owned();
        let value = attribute
            .decode_and_unescape_value(reader.decoder())?
            .into_owned();
        if key.len() > MAX_TEXT || value.len() > MAX_TEXT {
            bail!("DAT attribute exceeds 8192 characters");
        }
        values.insert(key, value);
    }
    Ok(values)
}

fn digest(attributes: &BTreeMap<String, String>, key: &str, len: usize) -> Result<Option<String>> {
    attributes
        .get(key)
        .map(|value| {
            if value.len() != len || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("Invalid {key} digest: expected {len} hexadecimal characters");
            }
            Ok(value.to_ascii_lowercase())
        })
        .transpose()
}

fn entry(attributes: &BTreeMap<String, String>, game: &str) -> Result<CatalogEntry> {
    let filename = attributes
        .get("name")
        .filter(|s| !s.is_empty())
        .context("ROM is missing a name")?
        .clone();
    let raw_status = attributes.get("status").cloned().unwrap_or_default();
    let quality = match raw_status.as_str() {
        "good" => DumpQuality::Good,
        "baddump" => DumpQuality::Suspect,
        "nodump" => DumpQuality::Undumped,
        "" => DumpQuality::Unspecified,
        _ => DumpQuality::Unspecified,
    };
    Ok(CatalogEntry {
        game: game.into(),
        filename,
        size: attributes
            .get("size")
            .map(|s| s.parse::<u64>())
            .transpose()
            .context("Invalid ROM size")?,
        digests: Digests {
            crc32: digest(attributes, "crc", 8)?,
            md5: digest(attributes, "md5", 32)?,
            sha1: digest(attributes, "sha1", 40)?,
            sha256: digest(attributes, "sha256", 64)?,
        },
        quality,
        raw_status,
        group_members: 1,
        unsupported_algorithms: attributes
            .keys()
            .filter(|key| {
                !matches!(
                    key.as_str(),
                    "name" | "size" | "crc" | "md5" | "sha1" | "sha256" | "status"
                )
            })
            .cloned()
            .collect(),
    })
}

/// Parses UTF-8 Logiqx datafiles without a DTD resolver or any network I/O.
/// External DOCTYPE identifiers are inert; internal subsets/entities are rejected.
pub fn parse_dat(
    bytes: &[u8],
    origin: &Path,
    system: &str,
    cancel: &Cancellation,
) -> Result<Catalog> {
    if bytes.len() as u64 > MAX_DAT_BYTES {
        bail!("DAT exceeds the 64 MiB import limit");
    }
    if system.trim().is_empty() {
        bail!("Select a system explicitly before importing");
    }
    std::str::from_utf8(bytes).context("Only UTF-8 DAT files are currently supported")?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let mut stack: Vec<String> = Vec::new();
    let mut root_seen = false;
    let mut root_closed = false;
    let mut header = BTreeMap::<String, String>::new();
    let mut game = None::<String>;
    let mut group_start = 0;
    let mut entries = Vec::new();
    loop {
        cancel.check()?;
        let event = reader.read_event().context("Malformed XML DAT")?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                let tag = std::str::from_utf8(start.name().as_ref())?.to_owned();
                if stack.is_empty() {
                    if root_seen || tag != "datafile" || empty {
                        bail!("Expected a single Logiqx <datafile> root");
                    }
                    root_seen = true;
                }
                if stack.len() >= 32 {
                    bail!("DAT nesting exceeds 32 levels");
                }
                let attrs = attributes(&start, &reader)?;
                if tag == "game" || tag == "machine" {
                    if stack.as_slice() != ["datafile"] {
                        bail!("Unsupported nested game/machine schema");
                    }
                    game = Some(
                        attrs
                            .get("name")
                            .filter(|s| !s.is_empty())
                            .context("Game is missing a name")?
                            .clone(),
                    );
                    group_start = entries.len();
                }
                if tag == "rom" {
                    if stack.len() != 2 || !matches!(stack[1].as_str(), "game" | "machine") {
                        bail!("ROM must be directly inside a game/machine");
                    }
                    if entries.len() >= MAX_ENTRIES {
                        bail!("DAT exceeds one million ROM entries");
                    }
                    entries.push(entry(&attrs, game.as_deref().context("ROM outside game")?)?);
                }
                if matches!(tag.as_str(), "disk" | "software" | "part" | "dataarea") {
                    bail!(
                        "Unsupported catalog schema; only loose-ROM Logiqx entries are supported"
                    );
                }
                if empty {
                    if tag == "game" || tag == "machine" {
                        game = None;
                    }
                } else {
                    stack.push(tag);
                }
            }
            Event::End(end) => {
                let tag = std::str::from_utf8(end.name().as_ref())?.to_owned();
                if stack.pop().as_deref() != Some(tag.as_str()) {
                    bail!("Unbalanced XML elements");
                }
                if tag == "game" || tag == "machine" {
                    let count = entries.len() - group_start;
                    for entry in &mut entries[group_start..] {
                        entry.group_members = count;
                    }
                    game = None;
                }
                if stack.is_empty() {
                    root_closed = true;
                }
            }
            Event::Text(text) => {
                let decoded = text.decode()?;
                let decoded = quick_xml::escape::unescape(&decoded)?;
                append_text(&stack, &mut header, &decoded)?;
            }
            Event::CData(text) => {
                append_text(&stack, &mut header, &text.decode()?)?;
            }
            Event::GeneralRef(reference) => {
                let escaped = format!("&{};", reference.decode()?);
                let decoded = quick_xml::escape::unescape(&escaped)
                    .context("Custom XML entities are unsupported")?;
                append_text(&stack, &mut header, &decoded)?;
            }
            Event::DocType(text) => {
                let text = text.decode()?;
                if root_seen
                    || text.contains('[')
                    || text.contains("ENTITY")
                    || !text.starts_with("datafile")
                {
                    bail!("Internal DTD subsets and custom entities are prohibited");
                }
            }
            Event::Decl(decl) => {
                if let Some(encoding) = decl.encoding()
                    && !encoding?.as_ref().eq_ignore_ascii_case(b"utf-8")
                {
                    bail!("Only UTF-8 XML is supported");
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_closed || !stack.is_empty() || entries.is_empty() {
        bail!("DAT is incomplete or contains no ROM entries");
    }
    let name = header
        .remove("name")
        .filter(|s| !s.trim().is_empty())
        .context("DAT header is missing a name")?;
    let version = header
        .remove("version")
        .unwrap_or_else(|| "unspecified".into());
    Ok(Catalog {
        name,
        version,
        entries,
        provenance: Provenance {
            provider: "Local import (publisher not authenticated)".into(),
            origin: origin.into(),
            system: system.trim().into(),
            imported_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            format: "Logiqx XML / UTF-8".into(),
            license_reference: None,
            license_review_date: None,
            snapshot_sha256: format!("{:x}", Sha256::digest(bytes)),
        },
    })
}

fn append_text(stack: &[String], header: &mut BTreeMap<String, String>, text: &str) -> Result<()> {
    if stack.is_empty() && !text.trim().is_empty() {
        bail!("Text outside the XML root");
    }
    if stack.len() == 3 && stack[0] == "datafile" && stack[1] == "header" {
        let value = header.entry(stack[2].clone()).or_default();
        if value.len() + text.len() > MAX_TEXT {
            bail!("DAT header text exceeds 8192 characters");
        }
        value.push_str(text);
    }
    Ok(())
}
