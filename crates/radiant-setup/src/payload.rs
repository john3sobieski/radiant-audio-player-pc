//! Payload format: `pack` appends the files to the end of the setup exe, the setup exe reads them from itself.
//!
//! File layout:  STUB (the exe as built by cargo) | PAYLOAD | FOOTER
//! PAYLOAD: u32 count, then per file: u16 name length, name (UTF-8), u64 raw size, u64 packed size, deflate data.
//! FOOTER (16 bytes): u64 length of STUB, 8 magic bytes.
//! The STUB alone (no footer) is what becomes `uninstall.exe`.

#![allow(dead_code)]

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

pub const MAGIC: &[u8; 8] = b"RDNTPAY1";
pub const FOOTER_LEN: u64 = 16;

/// One packed file.
pub struct Entry {
    pub name: String,
    pub raw_len: u64,
    packed_len: u64,
    /// Where the packed bytes start in the exe.
    offset: u64,
}

/// The payload of an exe: the list of files and the length of the stub in front of them.
pub struct Payload {
    pub stub_len: u64,
    pub entries: Vec<Entry>,
}

fn read_u16(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

impl Payload {
    /// Looks for a payload at the end of `exe`. `Ok(None)` = this exe has none (it is the stub itself).
    pub fn open(exe: &Path) -> io::Result<Option<Payload>> {
        let mut file = File::open(exe)?;
        let size = file.metadata()?.len();
        if size < FOOTER_LEN {
            return Ok(None);
        }
        file.seek(SeekFrom::Start(size - FOOTER_LEN))?;
        let stub_len = read_u64(&mut file)?;
        let mut magic = [0u8; 8];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC || stub_len >= size - FOOTER_LEN {
            return Ok(None);
        }

        file.seek(SeekFrom::Start(stub_len))?;
        let count = read_u32(&mut file)?;
        let mut entries = Vec::new();
        for _ in 0..count {
            let name_len = read_u16(&mut file)? as usize;
            let mut name = vec![0u8; name_len];
            file.read_exact(&mut name)?;
            let name = String::from_utf8(name).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad file name"))?;
            let raw_len = read_u64(&mut file)?;
            let packed_len = read_u64(&mut file)?;
            let offset = file.stream_position()?;
            file.seek(SeekFrom::Current(packed_len as i64))?;
            entries.push(Entry { name, raw_len, packed_len, offset });
        }
        Ok(Some(Payload { stub_len, entries }))
    }

    /// Total unpacked size of all files in bytes.
    pub fn raw_total(&self) -> u64 {
        self.entries.iter().map(|e| e.raw_len).sum()
    }

    /// Unpacks one file from `exe` to `dest` (through a `.part` file, so a half-written file never has the final name).
    pub fn extract(&self, exe: &Path, entry: &Entry, dest: &Path) -> io::Result<()> {
        let mut src = File::open(exe)?;
        src.seek(SeekFrom::Start(entry.offset))?;
        let mut decoder = DeflateDecoder::new(src.take(entry.packed_len));

        let mut part = dest.as_os_str().to_owned();
        part.push(".part");
        let part = std::path::PathBuf::from(part);
        let result = (|| {
            let mut out = File::create(&part)?;
            let copied = io::copy(&mut decoder, &mut out)?;
            if copied != entry.raw_len {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "the installer file is damaged"));
            }
            out.flush()
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        std::fs::rename(&part, dest)
    }
}

/// Writes `out` = the stub (`stub` file, as it is) + the files + the footer. Used by `pack`.
pub fn write_packed(stub: &Path, files: &[(String, std::path::PathBuf)], out: &Path) -> io::Result<()> {
    let mut stub_bytes = Vec::new();
    File::open(stub)?.read_to_end(&mut stub_bytes)?;

    let mut file = File::create(out)?;
    file.write_all(&stub_bytes)?;
    file.write_all(&(files.len() as u32).to_le_bytes())?;
    for (name, path) in files {
        let mut raw = Vec::new();
        File::open(path)?.read_to_end(&mut raw)?;
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&raw)?;
        let packed = encoder.finish()?;
        file.write_all(&(name.len() as u16).to_le_bytes())?;
        file.write_all(name.as_bytes())?;
        file.write_all(&(raw.len() as u64).to_le_bytes())?;
        file.write_all(&(packed.len() as u64).to_le_bytes())?;
        file.write_all(&packed)?;
    }
    file.write_all(&(stub_bytes.len() as u64).to_le_bytes())?;
    file.write_all(MAGIC)?;
    file.flush()
}

/// Copies the first `stub_len` bytes of `exe` (the program without payload) to `dest`.
pub fn write_stub(exe: &Path, stub_len: u64, dest: &Path) -> io::Result<()> {
    let src = File::open(exe)?;
    let mut limited = src.take(stub_len);
    let mut out = File::create(dest)?;
    let copied = io::copy(&mut limited, &mut out)?;
    if copied != stub_len {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "could not copy the uninstaller"));
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("radiant-setup-test-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pack_and_read_back() {
        let dir = temp("pack");
        let stub = dir.join("stub.exe");
        std::fs::write(&stub, b"MZ-stub-bytes").unwrap();
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&a, b"hello").unwrap();
        std::fs::write(&b, &big).unwrap();
        let out = dir.join("setup.exe");
        write_packed(&stub, &[("a.bin".into(), a), ("b.bin".into(), b)], &out).unwrap();

        let payload = Payload::open(&out).unwrap().expect("payload");
        assert_eq!(payload.stub_len, 13);
        assert_eq!(payload.entries.len(), 2);
        assert_eq!(payload.raw_total(), 5 + 200_000);

        let target = dir.join("b.out");
        payload.extract(&out, &payload.entries[1], &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), big);
        assert!(!dir.join("b.out.part").exists());

        // The stub copy is the program without payload.
        let copy = dir.join("uninstall.exe");
        write_stub(&out, payload.stub_len, &copy).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), b"MZ-stub-bytes");
        assert!(Payload::open(&copy).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_exe_has_no_payload() {
        let dir = temp("plain");
        let exe = dir.join("plain.exe");
        std::fs::write(&exe, vec![7u8; 1000]).unwrap();
        assert!(Payload::open(&exe).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
