//! GTA SA executable version detection, replicating CLEO's own check.
//!
//! CLEO's `CGameVersionManager` identifies the game by reading a 4-byte value at
//! a fixed virtual address and comparing it to a known signature. Those values
//! live in the executable's initialized data, so we can read them straight from
//! the exe file (mapping virtual address -> file offset through the PE section
//! table) and report the same verdict CLEO would reach before the game runs.
//! CLEO5 loads on US 1.0, EU 1.0/1.01 and Steam; an unrecognized exe is `GV_UNK`
//! and CLEO refuses to initialize.

use crate::prelude::*;

const MZ_SIGNATURE_LEN: usize = 2;
const PE_SIGNATURE_LEN: usize = 4;
const PE_MIN_DOS_HEADER_LEN: usize = 0x40;
const PE_OFFSET_POINTER_OFFSET: usize = 0x3C;
const COFF_HEADER_LEN: usize = 20;
const COFF_SECTION_COUNT_OFFSET: usize = 2;
const COFF_OPTIONAL_HEADER_SIZE_OFFSET: usize = 16;
const PE32_IMAGE_BASE_OFFSET: usize = 28;
const SECTION_HEADER_LEN: usize = 40;
const SECTION_VIRTUAL_SIZE_OFFSET: usize = 8;
const SECTION_VIRTUAL_ADDRESS_OFFSET: usize = 12;
const SECTION_RAW_POINTER_OFFSET: usize = 20;
const U16_BYTE_WIDTH: usize = 2;
const U32_BYTE_WIDTH: usize = 4;

const EU11_SIGNATURE_VA: u32 = 0x8A6168;
const EU11_SIGNATURE_VALUE: u32 = 0x8523A0;
const US_EU_SIGNATURE_VA: u32 = 0x8A4004;
const US10_SIGNATURE_VALUE: u32 = 0x8339CA;
const EU10_SIGNATURE_VALUE: u32 = 0x833A0A;
const STEAM_SIGNATURE_VA: u32 = 0x913000;
const STEAM_SIGNATURE_VALUE: u32 = 0x8A5B0C;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameVersion
{
    Us10,
    Eu10,
    Eu11,
    Steam,
    /// A valid PE, but none of CLEO's signatures matched -- CLEO would refuse it.
    Unrecognized,
    /// The file could not be read or parsed as a PE image.
    Unreadable,
}

impl fmt::Display for GameVersion
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self {
            GameVersion::Us10 => write!(f, "1.0 US"),
            GameVersion::Eu10 => write!(f, "1.0 EU"),
            GameVersion::Eu11 => write!(f, "1.01 EU"),
            GameVersion::Steam => write!(f, "steam (older, CLEO-compatible)"),
            GameVersion::Unrecognized => write!(f, "unrecognized"),
            GameVersion::Unreadable => write!(f, "unreadable"),
        };
    }
}

impl GameVersion
{
    /// Whether CLEO5 recognizes and supports this executable.
    pub(crate) fn is_cleo_supported(self) -> bool
    {
        return matches!(
            self,
            GameVersion::Us10 | GameVersion::Eu10 | GameVersion::Eu11 | GameVersion::Steam
        );
    }
}

/// Detect the version of a GTA SA executable file.
pub(crate) fn detect_game_version(exe_path: &Path) -> GameVersion
{
    return match fs::read(exe_path) {
        Ok(bytes) => detect_from_bytes(&bytes),
        Err(_) => GameVersion::Unreadable,
    };
}

/// The exact checks CLEO's `CGameVersionManager` performs, in the same order.
fn detect_from_bytes(bytes: &[u8]) -> GameVersion
{
    let Some(image) = PeImage::parse(bytes) else {
        return GameVersion::Unreadable;
    };
    if image.read_u32_at_va(EU11_SIGNATURE_VA) == Some(EU11_SIGNATURE_VALUE)
    {
        return GameVersion::Eu11;
    }
    match image.read_u32_at_va(US_EU_SIGNATURE_VA)
    {
        Some(US10_SIGNATURE_VALUE) => return GameVersion::Us10,
        Some(EU10_SIGNATURE_VALUE) => return GameVersion::Eu10,
        _ => {}
    }
    if image.read_u32_at_va(STEAM_SIGNATURE_VA) == Some(STEAM_SIGNATURE_VALUE)
    {
        return GameVersion::Steam;
    }
    return GameVersion::Unrecognized;
}

/// One PE section: where it maps in memory and where its bytes live in the file.
struct Section
{
    virtual_address: u32,
    virtual_size: u32,
    raw_pointer: u32,
}

/// A minimal 32-bit PE view -- just enough to translate a virtual address to a
/// file offset and read the 4 bytes there.
struct PeImage<'a>
{
    bytes: &'a [u8],
    image_base: u32,
    sections: Vec<Section>,
}

impl<'a> PeImage<'a>
{
    fn parse(bytes: &'a [u8]) -> Option<Self>
    {
        if bytes.len() < PE_MIN_DOS_HEADER_LEN || &bytes[0..MZ_SIGNATURE_LEN] != b"MZ"
        {
            return None;
        }
        let pe_offset = read_u32(bytes, PE_OFFSET_POINTER_OFFSET)? as usize;
        if &bytes.get(pe_offset..pe_offset + PE_SIGNATURE_LEN)? != b"PE\0\0"
        {
            return None;
        }
        let coff = pe_offset + PE_SIGNATURE_LEN;
        let section_count = read_u16(bytes, coff + COFF_SECTION_COUNT_OFFSET)? as usize;
        let optional_size = read_u16(bytes, coff + COFF_OPTIONAL_HEADER_SIZE_OFFSET)? as usize;
        let optional = coff + COFF_HEADER_LEN;
        // PE32 image base sits at optional-header offset 28.
        let image_base = read_u32(bytes, optional + PE32_IMAGE_BASE_OFFSET)?;
        let table = optional + optional_size;
        let mut sections = Vec::new();
        for index in 0..section_count
        {
            let entry = table + index * SECTION_HEADER_LEN;
            if bytes.len() < entry + SECTION_HEADER_LEN
            {
                break;
            }
            sections.push(Section {
                virtual_size: read_u32(bytes, entry + SECTION_VIRTUAL_SIZE_OFFSET)?,
                virtual_address: read_u32(bytes, entry + SECTION_VIRTUAL_ADDRESS_OFFSET)?,
                raw_pointer: read_u32(bytes, entry + SECTION_RAW_POINTER_OFFSET)?,
            });
        }
        return Some(PeImage {
            bytes,
            image_base,
            sections,
        });
    }

    /// Read the little-endian u32 at a virtual address, or `None` if it is not
    /// mapped by any section or lies outside the file.
    fn read_u32_at_va(&self, va: u32) -> Option<u32>
    {
        let rva = va.checked_sub(self.image_base)?;
        for section in &self.sections
        {
            let span = if section.virtual_size == 0 {
                // Some linkers leave virtual size zero; the raw size then governs.
                self.bytes.len() as u32
            } else {
                section.virtual_size
            };
            let end = section.virtual_address.checked_add(span)?;
            if rva >= section.virtual_address && rva < end
            {
                let offset = (rva - section.virtual_address).checked_add(section.raw_pointer)?;
                return read_u32(self.bytes, offset as usize);
            }
        }
        return None;
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32>
{
    let slice = bytes.get(offset..offset + U32_BYTE_WIDTH)?;
    let array = slice.try_into().ok()?;
    return Some(u32::from_le_bytes(array));
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16>
{
    let slice = bytes.get(offset..offset + U16_BYTE_WIDTH)?;
    let array = slice.try_into().ok()?;
    return Some(u16::from_le_bytes(array));
}

#[cfg(test)]
mod tests
{
    use super::*;

    const TEST_PE_OFFSET: usize = 0x80;
    const TEST_PE32_OPTIONAL_HEADER_SIZE: usize = 0xE0;
    const TEST_SECTION_RAW_POINTER: usize = 0x400;
    const TEST_SECTION_VIRTUAL_SIZE: usize = 0x1000;
    const TEST_SECTION_COUNT: u16 = 1;
    const TEST_IMAGE_BASE: u32 = 0x400000;
    const TEST_WRONG_SIGNATURE_VALUE: u32 = 0xDEADBEEF;

    #[test]
    fn detects_us_10_from_its_signature()
    {
        let pe = build_pe(TEST_IMAGE_BASE, US_EU_SIGNATURE_VA, US10_SIGNATURE_VALUE);
        assert_eq!(detect_from_bytes(&pe), GameVersion::Us10);
        assert!(detect_from_bytes(&pe).is_cleo_supported());
    }

    #[test]
    fn detects_steam_from_its_signature()
    {
        let pe = build_pe(TEST_IMAGE_BASE, STEAM_SIGNATURE_VA, STEAM_SIGNATURE_VALUE);
        assert_eq!(detect_from_bytes(&pe), GameVersion::Steam);
    }

    #[test]
    fn wrong_signature_is_unrecognized()
    {
        let pe = build_pe(
            TEST_IMAGE_BASE,
            US_EU_SIGNATURE_VA,
            TEST_WRONG_SIGNATURE_VALUE,
        );
        assert_eq!(detect_from_bytes(&pe), GameVersion::Unrecognized);
        assert!(!detect_from_bytes(&pe).is_cleo_supported());
    }

    #[test]
    fn non_pe_is_unreadable()
    {
        assert_eq!(detect_from_bytes(b"not an exe"), GameVersion::Unreadable);
    }

    /// Build a minimal but valid PE32 with one section whose start maps `sig_va`
    /// and stores `sig_val` there, so the detector can be exercised end to end.
    fn build_pe(image_base: u32, sig_va: u32, sig_val: u32) -> Vec<u8>
    {
        let pe_offset = TEST_PE_OFFSET;
        let optional_size = TEST_PE32_OPTIONAL_HEADER_SIZE;
        let table = pe_offset + PE_SIGNATURE_LEN + COFF_HEADER_LEN + optional_size;
        let raw_pointer = TEST_SECTION_RAW_POINTER;
        let mut bytes = vec![0u8; raw_pointer + TEST_SECTION_VIRTUAL_SIZE];
        bytes[0..MZ_SIGNATURE_LEN].copy_from_slice(b"MZ");
        bytes[PE_OFFSET_POINTER_OFFSET..PE_MIN_DOS_HEADER_LEN]
            .copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + PE_SIGNATURE_LEN].copy_from_slice(b"PE\0\0");
        let coff = pe_offset + PE_SIGNATURE_LEN;
        bytes[coff + COFF_SECTION_COUNT_OFFSET..coff + COFF_SECTION_COUNT_OFFSET + U16_BYTE_WIDTH]
            .copy_from_slice(&TEST_SECTION_COUNT.to_le_bytes());
        bytes[coff + COFF_OPTIONAL_HEADER_SIZE_OFFSET
            ..coff + COFF_OPTIONAL_HEADER_SIZE_OFFSET + U16_BYTE_WIDTH]
            .copy_from_slice(&(optional_size as u16).to_le_bytes());
        let optional = coff + COFF_HEADER_LEN;
        bytes
            [optional + PE32_IMAGE_BASE_OFFSET..optional + PE32_IMAGE_BASE_OFFSET + U32_BYTE_WIDTH]
            .copy_from_slice(&image_base.to_le_bytes());
        // Section starts exactly at sig_va, so the signature sits at its raw start.
        let section_va = sig_va - image_base;
        bytes[table + SECTION_VIRTUAL_SIZE_OFFSET
            ..table + SECTION_VIRTUAL_SIZE_OFFSET + U32_BYTE_WIDTH]
            .copy_from_slice(&(TEST_SECTION_VIRTUAL_SIZE as u32).to_le_bytes());
        bytes[table + SECTION_VIRTUAL_ADDRESS_OFFSET
            ..table + SECTION_VIRTUAL_ADDRESS_OFFSET + U32_BYTE_WIDTH]
            .copy_from_slice(&section_va.to_le_bytes());
        bytes[table + SECTION_RAW_POINTER_OFFSET
            ..table + SECTION_RAW_POINTER_OFFSET + U32_BYTE_WIDTH]
            .copy_from_slice(&(raw_pointer as u32).to_le_bytes());
        bytes[raw_pointer..raw_pointer + U32_BYTE_WIDTH].copy_from_slice(&sig_val.to_le_bytes());
        return bytes;
    }
}

