//! APK metadata for local files, read in-process: an APK's AndroidManifest.xml
//! is binary XML (AXML) and its label lives in resources.arsc, so the PC needs no
//! build-tools. The original pushed aapt-arm-pie into /data/local/tmp and ran it
//! on the TV to read labels and versions; reading them here covers the same need
//! (labelling a bulk sideload, checking what a downloaded APK really is) without
//! writing an executable into the device's tmp. Setting AAPT=/path/to/aapt2 uses
//! that binary instead.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq)]
pub struct Badging {
    pub package: String,
    pub label: String,
    pub version_name: String,
    pub version_code: String,
    pub sdk: String,
    pub target_sdk: String,
    pub launchable: Option<String>,
}

/// A manifest attribute: a literal, a reference into resources.arsc, or absent.
#[derive(Debug, PartialEq, Default, Clone)]
pub enum Attr {
    #[default]
    Missing,
    Text(String),
    Reference(u32),
}

impl Attr {
    /// The literal, or "@0x7f130000" when only the reference is known.
    pub fn text(&self) -> String {
        match self {
            Attr::Text(t) => t.clone(),
            Attr::Reference(id) => format!("@{id:#010x}"),
            Attr::Missing => String::new(),
        }
    }
}

/// The manifest fields this tool reports.
#[derive(Debug, PartialEq, Default)]
pub struct Manifest {
    pub package: String,
    pub version_code: Attr,
    pub version_name: Attr,
    pub min_sdk: Attr,
    pub target_sdk: Attr,
    pub label: Attr,
    pub launchable: Option<String>,
}

pub fn info(file: &Path) -> Result<Badging> {
    if !file.exists() {
        bail!("no such file: {}", file.display());
    }
    if let Ok(bin) = std::env::var("AAPT") {
        let bin = bin.trim().to_string();
        if !bin.is_empty() {
            return aapt_info(&bin, file);
        }
    }
    let data = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let manifest = zip_entry(&data, "AndroidManifest.xml")
        .ok_or_else(|| anyhow::anyhow!("no AndroidManifest.xml in {} - not an apk?", file.display()))?;
    let m = parse_manifest(&manifest)
        .ok_or_else(|| anyhow::anyhow!("unreadable manifest in {}", file.display()))?;
    // a version or label written as @string/x only becomes a value here
    let arsc = zip_entry(&data, "resources.arsc");
    let resolve = |a: &Attr| -> String {
        match a {
            Attr::Reference(id) => arsc
                .as_deref()
                .and_then(|blob| resolve_string(blob, *id))
                .unwrap_or_else(|| a.text()),
            _ => a.text(),
        }
    };
    Ok(Badging {
        package: m.package.clone(),
        label: resolve(&m.label),
        version_name: resolve(&m.version_name),
        version_code: resolve(&m.version_code),
        sdk: resolve(&m.min_sdk),
        target_sdk: resolve(&m.target_sdk),
        launchable: m.launchable.as_deref().map(|a| qualify(&m.package, a)),
    })
}

/// ".MainActivity" and "MainActivity" are relative to the package, as aapt prints them.
fn qualify(package: &str, activity: &str) -> String {
    if activity.starts_with('.') {
        format!("{package}{activity}")
    } else if activity.contains('.') {
        activity.to_string()
    } else {
        format!("{package}.{activity}")
    }
}

/// Every .apk in a directory, sorted - the bulk-sideload worklist.
pub fn apks(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().map(|x| x.eq_ignore_ascii_case("apk")).unwrap_or(false) {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------- aapt override

fn aapt_info(bin: &str, file: &Path) -> Result<Badging> {
    let out = Command::new(bin)
        .args(["dump", "badging", &file.to_string_lossy()])
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    parse_badging(&text).ok_or_else(|| anyhow::anyhow!("{bin} could not read {}", file.display()))
}

/// aapt dump badging output -> fields. Iterates lines in order so a label
/// containing a quote cannot derail the parser; aapt2 prints the same lines.
pub fn parse_badging(text: &str) -> Option<Badging> {
    let field = |prefix: &str| -> Option<String> {
        text.lines()
            .find(|l| l.starts_with(prefix))
            .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
    };
    let package_line = field("package:")?;
    let attr = |line: &str, key: &str| -> String {
        let needle = format!("{key}='");
        line.find(&needle)
            .map(|i| line[i + needle.len()..].split('\'').next().unwrap_or("").to_string())
            .unwrap_or_default()
    };
    let launchable = text
        .lines()
        .find(|l| l.starts_with("launchable-activity:"))
        .map(|l| attr(l, "name"))
        .filter(|s| !s.is_empty());
    Some(Badging {
        package: attr(&package_line, "name"),
        version_code: attr(&package_line, "versionCode"),
        version_name: attr(&package_line, "versionName"),
        label: field("application-label:").unwrap_or_default().trim_matches('\'').to_string(),
        // aapt quotes numeric fields: sdkVersion:'21'
        sdk: field("sdkVersion:").unwrap_or_default().trim_matches('\'').to_string(),
        target_sdk: field("targetSdkVersion:").unwrap_or_default().trim_matches('\'').to_string(),
        launchable,
    })
}

// ------------------------------------------------------------------------- zip

fn le16(d: &[u8], at: usize) -> Option<usize> {
    let b = d.get(at..at + 2)?;
    Some(usize::from(u16::from_le_bytes([b[0], b[1]])))
}

fn le32(d: &[u8], at: usize) -> Option<usize> {
    let b = d.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

/// One entry's bytes, walking the central directory so a deflated entry costs only
/// its own inflate. Stored and deflated entries only - zip64 is not an APK case.
fn zip_entry(data: &[u8], want: &str) -> Option<Vec<u8>> {
    let eocd = find_eocd(data)?;
    let count = le16(data, eocd + 10)?;
    let mut p = le32(data, eocd + 16)?;
    for _ in 0..count {
        if le32(data, p)? != 0x0201_4b50 {
            return None;
        }
        let method = le16(data, p + 10)?;
        let csize = le32(data, p + 20)?;
        let namelen = le16(data, p + 28)?;
        let extralen = le16(data, p + 30)?;
        let commentlen = le16(data, p + 32)?;
        let name = std::str::from_utf8(data.get(p + 46..p + 46 + namelen)?).ok()?;
        if name == want {
            let local = le32(data, p + 42)?;
            let start = local + 30 + le16(data, local + 26)? + le16(data, local + 28)?;
            let raw = data.get(start..start + csize)?;
            return match method {
                0 => Some(raw.to_vec()),
                8 => miniz_oxide::inflate::decompress_to_vec(raw).ok(),
                _ => None,
            };
        }
        p += 46 + namelen + extralen + commentlen;
    }
    None
}

/// The end-of-central-directory record, found from the tail (it may carry a comment).
fn find_eocd(data: &[u8]) -> Option<usize> {
    let first = data.len().saturating_sub(66_000);
    // an EOCD with no comment starts exactly 22 bytes from the end
    let last = data.len().saturating_sub(22);
    (first..=last).rev().find(|&i| le32(data, i) == Some(0x0605_4b50))
}

// ------------------------------------------------------------------------ AXML

const RES_STRING_POOL: usize = 0x0001;
const RES_XML_TYPE: usize = 0x0003;
const RES_XML_START_ELEMENT: usize = 0x0102;
const RES_XML_END_ELEMENT: usize = 0x0103;
const TYPE_STRING: u8 = 0x03;
const TYPE_REFERENCE: u8 = 0x01;
const TYPE_INT_DEC: u8 = 0x10;
const TYPE_INT_HEX: u8 = 0x11;

/// AndroidManifest.xml (binary XML) -> the fields above. None when the bytes are
/// not an AXML document or the string pool cannot be read.
pub fn parse_manifest(data: &[u8]) -> Option<Manifest> {
    if le16(data, 0)? != RES_XML_TYPE {
        return None;
    }
    let strings = string_pool(data, le16(data, 2)?)?;
    let get = |i: usize| strings.get(i).cloned().unwrap_or_default();
    let mut m = Manifest::default();
    let mut activity: Option<String> = None;
    let mut filter = (false, false); // MAIN, LAUNCHER seen in the open intent-filter
    let mut p = le16(data, 2)?;
    while p + 8 <= data.len() {
        let typ = le16(data, p)?;
        let size = le32(data, p + 4)?;
        if size == 0 {
            break;
        }
        match typ {
            RES_XML_START_ELEMENT => {
                let name = get(le32(data, p + 20)?);
                let attrs = attributes(data, p, &strings)?;
                let attr = |key: &str| attrs.iter().find(|(n, _)| n == key).map(|(_, v)| v.clone());
                match name.as_str() {
                    "manifest" => {
                        m.package = attr("package").unwrap_or_default().text();
                        m.version_code = attr("versionCode").unwrap_or_default();
                        m.version_name = attr("versionName").unwrap_or_default();
                    }
                    "uses-sdk" => {
                        m.min_sdk = attr("minSdkVersion").unwrap_or_default();
                        m.target_sdk = attr("targetSdkVersion").unwrap_or_default();
                    }
                    "application" => m.label = attr("label").unwrap_or_default(),
                    "activity" => {
                        activity = Some(attr("name").unwrap_or_default().text());
                        filter = (false, false);
                    }
                    "intent-filter" => filter = (false, false),
                    "action" => {
                        if attr("name").map(|a| a.text() == "android.intent.action.MAIN").unwrap_or(false) {
                            filter.0 = true;
                        }
                    }
                    "category" => {
                        if attr("name").map(|a| a.text() == "android.intent.category.LAUNCHER").unwrap_or(false) {
                            filter.1 = true;
                        }
                    }
                    _ => {}
                }
            }
            RES_XML_END_ELEMENT => {
                let name = get(le32(data, p + 20)?);
                match name.as_str() {
                    "intent-filter" => {
                        if filter.0 && filter.1 && m.launchable.is_none() {
                            m.launchable = activity.clone();
                        }
                        filter = (false, false);
                    }
                    "activity" => {
                        activity = None;
                        filter = (false, false);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        p += size;
    }
    Some(m)
}

/// The attributes of one start-element chunk: name plus typed value.
fn attributes(data: &[u8], at: usize, strings: &[String]) -> Option<Vec<(String, Attr)>> {
    // node header (16) then ResXMLTree_attrExt, whose attributeStart is relative to itself
    let base = at + le16(data, at + 2)? + le16(data, at + 24)?;
    let count = le16(data, at + 28)?;
    let stride = le16(data, at + 26)?.max(20);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let a = base + i * stride;
        let name = strings.get(le32(data, a + 4)?).cloned().unwrap_or_default();
        let dtype = *data.get(a + 15)?;
        let raw = le32(data, a + 16)?;
        let value = match dtype {
            TYPE_STRING => Attr::Text(strings.get(raw).cloned().unwrap_or_default()),
            TYPE_REFERENCE => Attr::Reference(raw as u32),
            TYPE_INT_DEC | TYPE_INT_HEX => Attr::Text(raw.to_string()),
            _ => Attr::Missing,
        };
        out.push((name, value));
    }
    Some(out)
}

// -------------------------------------------------------------- resources.arsc

const RES_TABLE_TYPE: usize = 0x0002;
const RES_TABLE_PACKAGE: usize = 0x0200;
const RES_TABLE_TYPE_SPEC: usize = 0x0201;

/// A resource id -> its default-configuration string, for @string/app_name labels.
pub fn resolve_string(arsc: &[u8], res_id: u32) -> Option<String> {
    if le16(arsc, 0)? != RES_TABLE_TYPE {
        return None;
    }
    let global = string_pool(arsc, le16(arsc, 2)?)?;
    let want_pkg = (res_id >> 24) as u8 as usize;
    let want_type = ((res_id >> 16) & 0xff) as usize;
    let want_entry = (res_id & 0xffff) as usize;
    let mut found: Option<String> = None;
    let mut p = le16(arsc, 2)?;
    while p + 8 <= arsc.len() {
        let size = le32(arsc, p + 4)?;
        if size == 0 {
            break;
        }
        if le16(arsc, p)? == RES_TABLE_PACKAGE && le32(arsc, p + 8)? == want_pkg {
            let mut q = p + le16(arsc, p + 2)?;
            let end = p + size;
            while q + 8 <= end {
                let inner = le32(arsc, q + 4)?;
                if inner == 0 {
                    break;
                }
                if le16(arsc, q)? == RES_TABLE_TYPE_SPEC && le32(arsc, q + 8)? == want_type {
                    if let Some((v, default)) = type_entry(arsc, q, want_entry, &global) {
                        if default {
                            return Some(v);
                        }
                        found = found.or(Some(v)); // a qualified variant, used only as fallback
                    }
                }
                q += inner;
            }
        }
        p += size;
    }
    found
}

/// One entry of a type chunk: (value, is that the default configuration).
/// The default config is the one with no locale, density or sdk qualifier set;
/// ResTable_config's own 4 byte size field is not part of that comparison.
fn type_entry(arsc: &[u8], at: usize, entry: usize, global: &[String]) -> Option<(String, bool)> {
    let hs = le16(arsc, at + 2)?;
    let count = le32(arsc, at + 12)?;
    if entry >= count {
        return None;
    }
    let cfg_size = le16(arsc, at + 20)?.max(4);
    let default = !arsc.get(at + 24..at + 20 + cfg_size)?.iter().any(|b| *b != 0);
    let offset = le32(arsc, at + hs + entry * 4)?;
    let e = at + le32(arsc, at + 16)? + offset;
    let esize = le16(arsc, e)?;
    if le16(arsc, e + 2)? & 1 != 0 {
        return None; // complex (bag) entry, no simple value
    }
    if *arsc.get(e + esize + 3)? == TYPE_STRING {
        return global.get(le32(arsc, e + esize + 4)?).cloned().map(|v| (v, default));
    }
    None
}

// ------------------------------------------------------------- string pool

/// A ResStringPool chunk (UTF-8 or UTF-16) -> its strings.
fn string_pool(data: &[u8], at: usize) -> Option<Vec<String>> {
    if le16(data, at)? != RES_STRING_POOL {
        return None;
    }
    let count = le32(data, at + 8)?;
    let utf8 = le32(data, at + 16)? & 0x100 != 0;
    let start = le32(data, at + 20)?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = le32(data, at + 28 + i * 4)?;
        out.push(read_string(data, at + start + off, utf8)?);
    }
    Some(out)
}

fn read_string(data: &[u8], at: usize, utf8: bool) -> Option<String> {
    if utf8 {
        let (_, p) = var_len(data, at)?; // utf16 length, unchecked either way
        let (n, p) = var_len(data, p)?;
        let bytes = data.get(p..p + n)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else {
        let first = le16(data, at)?;
        let (n, p) = if first & 0x8000 != 0 {
            ((first & 0x7fff) << 16 | le16(data, at + 2)?, at + 4)
        } else {
            (first, at + 2)
        };
        let raw = data.get(p..p + n * 2)?;
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units))
    }
}

/// 1 or 2 byte length prefix -> (value, offset after it).
fn var_len(data: &[u8], at: usize) -> Option<(usize, usize)> {
    let b = *data.get(at)?;
    if b & 0x80 != 0 {
        Some((usize::from(b & 0x7f) << 8 | le16(data, at + 1)?, at + 2))
    } else {
        Some((usize::from(b), at + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "package: name='com.example.tv' versionCode='42' versionName='1.2.3' platformBuildVersionName='14'\n\
sdkVersion:'21'\ntargetSdkVersion:'33'\nuses-permission: name='android.permission.INTERNET'\n\
application-label:'Example TV'\napplication-icon-320:'res/mipmap.png'\n\
launchable-activity: name='com.example.tv.MainActivity'  label='' icon=''\n";

    #[test]
    fn badging_fields_are_extracted() {
        let b = parse_badging(SAMPLE).unwrap();
        assert_eq!(b.package, "com.example.tv");
        assert_eq!(b.version_name, "1.2.3");
        assert_eq!(b.version_code, "42");
        assert_eq!(b.label, "Example TV");
        assert_eq!(b.sdk, "21");
        assert_eq!(b.target_sdk, "33");
        assert_eq!(b.launchable.as_deref(), Some("com.example.tv.MainActivity"));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(parse_badging("not an apk").is_none());
        assert!(parse_manifest(b"not an apk").is_none());
    }

    // ---- AXML fixture writer, so the parser is tested without build-tools ----

    const NAMES: &[&str] = &[
        "manifest", "package", "versionCode", "versionName", "uses-sdk", "minSdkVersion",
        "targetSdkVersion", "application", "label", "Example TV", "activity", "name",
        "com.example.tv.Settings", "com.example.tv.MainActivity", "intent-filter", "action",
        "android.intent.action.MAIN", "category", "android.intent.category.LAUNCHER",
    ];
    const VALS: &[&str] = &["com.example.tv", "1.2.3"];

    /// One pool: NAMES first, then VALS, so both name and value indices are stable.
    fn idx(s: &str) -> u32 {
        NAMES.iter().chain(VALS.iter()).position(|n| *n == s).unwrap() as u32
    }
    fn text(name: &str, value: &str) -> (u32, u32, u8) {
        (idx(name), idx(value), TYPE_STRING)
    }
    fn int(name: &str, v: u32) -> (u32, u32, u8) {
        (idx(name), v, TYPE_INT_DEC)
    }

    /// A ResStringPool chunk in the UTF-8 form aapt writes.
    fn pool(strings: &[&str]) -> Vec<u8> {
        let mut body: Vec<u8> = Vec::new();
        let mut offsets: Vec<u32> = Vec::new();
        for s in strings {
            offsets.push(body.len() as u32);
            body.push(s.len() as u8); // utf16 length
            body.push(s.len() as u8); // byte length
            body.extend_from_slice(s.as_bytes());
            body.push(0);
        }
        let start = 28 + strings.len() * 4;
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&1u16.to_le_bytes()); // RES_STRING_POOL_TYPE
        out.extend_from_slice(&28u16.to_le_bytes()); // headerSize
        out.extend_from_slice(&((start + body.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // styleCount
        out.extend_from_slice(&0x100u32.to_le_bytes()); // UTF8_FLAG
        out.extend_from_slice(&(start as u32).to_le_bytes()); // stringsStart
        out.extend_from_slice(&0u32.to_le_bytes()); // stylesStart
        for o in &offsets {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&body);
        out
    }

    /// A start or end element chunk with its attributes.
    fn element(name: u32, attrs: &[(u32, u32, u8)], end: bool) -> Vec<u8> {
        let hs: usize = 16;
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&(if end { 0x0103u16 } else { 0x0102u16 }).to_le_bytes());
        out.extend_from_slice(&(hs as u16).to_le_bytes());
        out.extend_from_slice(&((hs + 20 + attrs.len() * 20) as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // lineNumber
        out.extend_from_slice(&0xffffffffu32.to_le_bytes()); // comment
        out.extend_from_slice(&0xffffffffu32.to_le_bytes()); // ns
        out.extend_from_slice(&name.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // attributeStart
        out.extend_from_slice(&20u16.to_le_bytes()); // attributeSize
        out.extend_from_slice(&(attrs.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // idIndex
        out.extend_from_slice(&0u16.to_le_bytes()); // classIndex
        out.extend_from_slice(&0u16.to_le_bytes()); // styleIndex
        for (n, data, dtype) in attrs {
            out.extend_from_slice(&0xffffffffu32.to_le_bytes()); // ns
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&0xffffffffu32.to_le_bytes()); // rawValue
            out.extend_from_slice(&8u16.to_le_bytes()); // value size
            out.push(0);
            out.push(*dtype);
            out.extend_from_slice(&data.to_le_bytes());
        }
        out
    }

    /// A manifest with literal attributes and two activities; only the second has a
    /// MAIN/LAUNCHER intent-filter.
    fn manifest_fixture() -> Vec<u8> {
        let n = |s: &str| idx(s);
        let mut body: Vec<u8> = Vec::new();
        body.extend(element(n("manifest"), &[
            text("package", "com.example.tv"),
            int("versionCode", 42),
            text("versionName", "1.2.3"),
        ], false));
        body.extend(element(n("uses-sdk"), &[int("minSdkVersion", 21), int("targetSdkVersion", 33)], false));
        body.extend(element(n("application"), &[text("label", "Example TV")], false));
        body.extend(element(n("activity"), &[text("name", "com.example.tv.Settings")], false));
        body.extend(element(n("activity"), &[], true));
        body.extend(element(n("activity"), &[text("name", "com.example.tv.MainActivity")], false));
        body.extend(element(n("intent-filter"), &[], false));
        body.extend(element(n("action"), &[text("name", "android.intent.action.MAIN")], false));
        body.extend(element(n("action"), &[], true));
        body.extend(element(n("category"), &[text("name", "android.intent.category.LAUNCHER")], false));
        body.extend(element(n("category"), &[], true));
        body.extend(element(n("intent-filter"), &[], true));
        body.extend(element(n("activity"), &[], true));

        let mut all: Vec<&str> = NAMES.to_vec();
        all.extend_from_slice(VALS);
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&3u16.to_le_bytes()); // RES_XML_TYPE
        out.extend_from_slice(&8u16.to_le_bytes()); // headerSize
        out.extend_from_slice(&0u32.to_le_bytes()); // size, patched below
        out.extend_from_slice(&pool(&all));
        out.extend_from_slice(&body);
        let size = out.len() as u32;
        out[4..8].copy_from_slice(&size.to_le_bytes());
        out
    }

    #[test]
    fn manifest_attributes_and_launch_activity() {
        let m = parse_manifest(&manifest_fixture()).unwrap();
        assert_eq!(m.package, "com.example.tv");
        assert_eq!(m.version_code, Attr::Text("42".into()));
        assert_eq!(m.version_name, Attr::Text("1.2.3".into()));
        assert_eq!(m.min_sdk, Attr::Text("21".into()));
        assert_eq!(m.target_sdk, Attr::Text("33".into()));
        assert_eq!(m.label, Attr::Text("Example TV".into()));
        assert_eq!(m.launchable.as_deref(), Some("com.example.tv.MainActivity"));
    }

    #[test]
    fn a_relative_launch_activity_gets_the_package_prefix() {
        assert_eq!(qualify("com.example.tv", ".MainActivity"), "com.example.tv.MainActivity");
        assert_eq!(qualify("com.example.tv", "MainActivity"), "com.example.tv.MainActivity");
        assert_eq!(qualify("com.example.tv", "a.b.C"), "a.b.C");
    }

    /// resources.arsc with one package, one type and one default-config string entry.
    fn arsc_fixture() -> Vec<u8> {
        let global = pool(&["Example TV"]);
        // package chunk: header (id at +8) then the type chunk
        let entry: Vec<u8> = {
            let mut e: Vec<u8> = Vec::new();
            e.extend_from_slice(&8u16.to_le_bytes()); // entry size
            e.extend_from_slice(&0u16.to_le_bytes()); // flags
            e.extend_from_slice(&0u32.to_le_bytes()); // key
            e.extend_from_slice(&8u16.to_le_bytes()); // value size
            e.push(0);
            e.push(TYPE_STRING);
            e.extend_from_slice(&0u32.to_le_bytes()); // -> global pool index 0
            e
        };
        let type_chunk: Vec<u8> = {
            let cfg_size = 28usize;
            let hs = 20 + cfg_size;
            let mut t: Vec<u8> = Vec::new();
            t.extend_from_slice(&(RES_TABLE_TYPE_SPEC as u16).to_le_bytes());
            t.extend_from_slice(&(hs as u16).to_le_bytes());
            t.extend_from_slice(&0u32.to_le_bytes()); // size, patched below
            t.push(0x0e); // type id
            t.push(0);
            t.extend_from_slice(&0u16.to_le_bytes());
            t.extend_from_slice(&1u32.to_le_bytes()); // entryCount
            t.extend_from_slice(&((hs + 4) as u32).to_le_bytes()); // entriesStart
            t.extend(vec![0u8; cfg_size]); // default config
            t.extend_from_slice(&0u32.to_le_bytes()); // one entry offset
            t.extend_from_slice(&entry);
            let size = t.len() as u32;
            t[4..8].copy_from_slice(&size.to_le_bytes());
            t
        };
        let mut pkg: Vec<u8> = Vec::new();
        pkg.extend_from_slice(&(RES_TABLE_PACKAGE as u16).to_le_bytes());
        pkg.extend_from_slice(&12u16.to_le_bytes()); // headerSize (id sits at +8)
        pkg.extend_from_slice(&0u32.to_le_bytes()); // size, patched below
        pkg.extend_from_slice(&0x7fu32.to_le_bytes()); // package id
        pkg.extend_from_slice(&type_chunk);
        let psize = pkg.len() as u32;
        pkg[4..8].copy_from_slice(&psize.to_le_bytes());

        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&(RES_TABLE_TYPE as u16).to_le_bytes());
        out.extend_from_slice(&12u16.to_le_bytes()); // headerSize
        out.extend_from_slice(&0u32.to_le_bytes()); // size, patched below
        out.extend_from_slice(&1u32.to_le_bytes()); // packageCount
        out.extend_from_slice(&global);
        out.extend_from_slice(&pkg);
        let size = out.len() as u32;
        out[4..8].copy_from_slice(&size.to_le_bytes());
        out
    }

    #[test]
    fn a_string_resource_resolves_through_the_arsc() {
        let arsc = arsc_fixture();
        assert_eq!(resolve_string(&arsc, 0x7f0e0000).as_deref(), Some("Example TV"));
        assert_eq!(resolve_string(&arsc, 0x7f0e0001), None); // entry out of range
        assert_eq!(resolve_string(&arsc, 0x7e0e0000), None); // other package
    }

    #[test]
    fn only_stored_and_deflated_zip_entries_are_read() {
        // a stored entry: local + central records only, no compression
        let payload = b"AndroidManifest.xml-payload";
        let name = b"AndroidManifest.xml";
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 0, 0]); // version, flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // time, date, crc
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(payload);
        let cd = out.len();
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 20, 0]);
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // modtime, moddate, crc
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
        out.extend_from_slice(name);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
        out.extend_from_slice(&1u16.to_le_bytes()); // entries in total
        out.extend_from_slice(&((46 + name.len()) as u32).to_le_bytes()); // one central entry
        out.extend_from_slice(&(cd as u32).to_le_bytes()); // cd offset
        out.extend_from_slice(&0u16.to_le_bytes()); // comment
        assert_eq!(zip_entry(&out, "AndroidManifest.xml").as_deref(), Some(&payload[..]));
        assert_eq!(zip_entry(&out, "resources.arsc"), None);
    }
}
