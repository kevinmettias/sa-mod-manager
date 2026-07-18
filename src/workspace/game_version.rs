//! GTA SA executable version detection, replicating CLEO's own check.
//!
//! CLEO's `CGameVersionManager` identifies the game by reading a 4-byte value at
//! a fixed virtual address and comparing it to a known signature. Those values
//! live in the executable's initialized data, so we can read them straight from
//! the exe file (mapping virtual address → file offset through the PE section
//! table) and report the same verdict CLEO would reach — before the game runs.
//! CLEO5 loads on US 1.0, EU 1.0/1.01 and Steam; an unrecognized exe is `GV_UNK`
//! and CLEO refuses to initialize.

use crate::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameVersion {
    Us10,
    Eu10,
    Eu11,
    Steam,
    /// A valid PE, but none of CLEO's signatures matched — CLEO would refuse it.
    Unrecognized,
    /// The file could not be read or parsed as a PE image.
    Unreadable,
}

impl fmt::Display for GameVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameVersion::Us10 => write!(f, "1.0 US"),
            GameVersion::Eu10 => write!(f, "1.0 EU"),
            GameVersion::Eu11 => write!(f, "1.01 EU"),
            GameVersion::Steam => write!(f, "Steam (older, CLEO-compatible)"),
            GameVersion::Unrecognized => write!(f, "unrecognized"),
            GameVersion::Unreadable => write!(f, "unreadable"),
        }
    }
}

impl GameVersion {
    /// Whether CLEO5 recognizes and supports this executable.
    pub(crate) fn is_cleo_supported(self) -> bool {
        matches!(
            self,
            GameVersion::Us10 | GameVersion::Eu10 | GameVersion::Eu11 | GameVersion::Steam
        )
    }
}

/// Detect the version of a GTA SA executable file.
pub(crate) fn detect_game_version(exe_path: &Path) -> GameVersion {
    match fs::read(exe_path) {
        Ok(bytes) => detect_from_bytes(&bytes),
        Err(_) => GameVersion::Unreadable,
    }
}

/// The exact checks CLEO's `CGameVersionManager` performs, in the same order.
fn detect_from_bytes(bytes: &[u8]) -> GameVersion {
    let Some(image) = PeImage::parse(bytes) else {
        return GameVersion::Unreadable;
    };
    if image.read_u32_at_va(0x8A6168) == Some(0x8523A0) {
        return GameVersion::Eu11;
    }
    match image.read_u32_at_va(0x8A4004) {
        Some(0x8339CA) => return GameVersion::Us10,
        Some(0x833A0A) => return GameVersion::Eu10,
        _ => {}
    }
    if image.read_u32_at_va(0x913000) == Some(0x8A5B0C) {
        return GameVersion::Steam;
    }
    GameVersion::Unrecognized
}

/// One PE section: where it maps in memory and where its bytes live in the file.
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_pointer: u32,
}

/// A minimal 32-bit PE view — just enough to translate a virtual address to a
/// file offset and read the 4 bytes there.
struct PeImage<'a> {
    bytes: &'a [u8],
    image_base: u32,
    sections: Vec<Section>,
}

impl<'a> PeImage<'a> {
    fn parse(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
            return None;
        }
        let pe_offset = read_u32(bytes, 0x3C)? as usize;
        if &bytes.get(pe_offset..pe_offset + 4)? != b"PE\0\0" {
            return None;
        }
        let coff = pe_offset + 4;
        let section_count = read_u16(bytes, coff + 2)? as usize;
        let optional_size = read_u16(bytes, coff + 16)? as usize;
        let optional = coff + 20;
        // PE32 image base sits at optional-header offset 28.
        let image_base = read_u32(bytes, optional + 28)?;
        let table = optional + optional_size;
        let mut sections = Vec::new();
        for index in 0..section_count {
            let entry = table + index * 40;
            if bytes.len() < entry + 40 {
                break;
            }
            sections.push(Section {
                virtual_size: read_u32(bytes, entry + 8)?,
                virtual_address: read_u32(bytes, entry + 12)?,
                raw_pointer: read_u32(bytes, entry + 20)?,
            });
        }
        Some(PeImage {
            bytes,
            image_base,
            sections,
        })
    }

    /// Read the little-endian u32 at a virtual address, or `None` if it is not
    /// mapped by any section or lies outside the file.
    fn read_u32_at_va(&self, va: u32) -> Option<u32> {
        let rva = va.checked_sub(self.image_base)?;
        for section in &self.sections {
            let span = if section.virtual_size == 0 {
                // Some linkers leave virtual size zero; the raw size then governs.
                self.bytes.len() as u32
            } else {
                section.virtual_size
            };
            let end = section.virtual_address.checked_add(span)?;
            if rva >= section.virtual_address && rva < end {
                let offset = (rva - section.virtual_address).checked_add(section.raw_pointer)?;
                return read_u32(self.bytes, offset as usize);
            }
        }
        None
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but valid PE32 with one section whose start maps `sig_va`
    /// and stores `sig_val` there, so the detector can be exercised end to end.
    fn build_pe(image_base: u32, sig_va: u32, sig_val: u32) -> Vec<u8> {
        let pe_offset = 0x80usize;
        let optional_size = 0xE0usize; // standard PE32 optional header size
        let table = pe_offset + 4 + 20 + optional_size;
        let raw_pointer = 0x400usize;
        let mut bytes = vec![0u8; raw_pointer + 0x1000];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        let coff = pe_offset + 4;
        bytes[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes()); // 1 section
        bytes[coff + 16..coff + 18].copy_from_slice(&(optional_size as u16).to_le_bytes());
        let optional = coff + 20;
        bytes[optional + 28..optional + 32].copy_from_slice(&image_base.to_le_bytes());
        // Section starts exactly at sig_va, so the signature sits at its raw start.
        let section_va = sig_va - image_base;
        bytes[table + 8..table + 12].copy_from_slice(&0x1000u32.to_le_bytes()); // virtual size
        bytes[table + 12..table + 16].copy_from_slice(&section_va.to_le_bytes());
        bytes[table + 20..table + 24].copy_from_slice(&(raw_pointer as u32).to_le_bytes());
        bytes[raw_pointer..raw_pointer + 4].copy_from_slice(&sig_val.to_le_bytes());
        bytes
    }

    #[test]
    fn detects_us_10_from_its_signature() {
        // US 1.0: value 0x8339CA at VA 0x8A4004 (image base 0x400000 → RVA 0x4A4004).
        let pe = build_pe(0x400000, 0x8A4004, 0x8339CA);
        assert_eq!(detect_from_bytes(&pe), GameVersion::Us10);
        assert!(detect_from_bytes(&pe).is_cleo_supported());
    }

    #[test]
    fn detects_steam_from_its_signature() {
        let pe = build_pe(0x400000, 0x913000, 0x8A5B0C);
        assert_eq!(detect_from_bytes(&pe), GameVersion::Steam);
    }

    #[test]
    fn wrong_signature_is_unrecognized() {
        let pe = build_pe(0x400000, 0x8A4004, 0xDEADBEEF);
        assert_eq!(detect_from_bytes(&pe), GameVersion::Unrecognized);
        assert!(!detect_from_bytes(&pe).is_cleo_supported());
    }

    #[test]
    fn non_pe_is_unreadable() {
        assert_eq!(detect_from_bytes(b"not an exe"), GameVersion::Unreadable);
    }
}
