//! Pure-Rust Win64 COFF `.rsrc` Multi-Binary Icon Compiler (Zero External Tools!)
//! Works natively on BOTH `x86_64-pc-windows-gnu` (MinGW ld) AND `x86_64-pc-windows-msvc` (link.exe)!
//! Compiles each AuraOS binary's multi-resolution `.ico` directly into a standard x64 COFF `.o`
//! object containing `.rsrc` (RT_ICON + RT_GROUP_ICON ID=1) and links each binary specifically via
//! `cargo:rustc-link-arg-bin=<BIN>=<path.o>`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn push_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn pad4(buf: &mut Vec<u8>) {
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

fn write_rsrc_dir_header(buf: &mut Vec<u8>, id_count: u16) {
    push_u32(buf, 0); // Characteristics
    push_u32(buf, 0); // TimeDateStamp
    push_u16(buf, 0); // MajorVersion
    push_u16(buf, 0); // MinorVersion
    push_u16(buf, 0); // NumberOfNamedEntries
    push_u16(buf, id_count);
}

fn patch_u32(buf: &mut [u8], pos: usize, val: u32) {
    buf[pos..pos + 4].copy_from_slice(&val.to_le_bytes());
}

fn build_ico_coff(ico_path: &Path, obj_path: &Path) -> Result<(), String> {
    let ico = fs::read(ico_path).map_err(|e| format!("read {:?}: {}", ico_path, e))?;
    if ico.len() < 6 {
        return Err("ICO too small".into());
    }
    let reserved = u16::from_le_bytes([ico[0], ico[1]]);
    let ico_type = u16::from_le_bytes([ico[2], ico[3]]);
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    if reserved != 0 || ico_type != 1 || count == 0 || ico.len() < 6 + count * 16 {
        return Err("Invalid ICO header".into());
    }

    let mut icon_blobs: Vec<&[u8]> = Vec::with_capacity(count);
    let mut grp_blob = Vec::with_capacity(6 + count * 14);
    push_u16(&mut grp_blob, 0);
    push_u16(&mut grp_blob, 1);
    push_u16(&mut grp_blob, count as u16);

    for i in 0..count {
        let base = 6 + i * 16;
        let bytes_in_res = u32::from_le_bytes([ico[base + 8], ico[base + 9], ico[base + 10], ico[base + 11]]) as usize;
        let img_offset = u32::from_le_bytes([ico[base + 12], ico[base + 13], ico[base + 14], ico[base + 15]]) as usize;
        if img_offset + bytes_in_res > ico.len() {
            return Err(format!("ICO entry {} out of bounds", i));
        }
        icon_blobs.push(&ico[img_offset..img_offset + bytes_in_res]);
        grp_blob.extend_from_slice(&ico[base..base + 12]);
        push_u16(&mut grp_blob, (i + 1) as u16);
    }

    // Build `.rsrc` 3-level resource directory tree:
    // Level 1: Root -> [RT_ICON (3), RT_GROUP_ICON (14)]
    // Level 2: RT_ICON -> [1..=count], RT_GROUP_ICON -> [1]
    // Level 3: Lang 0x0409 -> IMAGE_RESOURCE_DATA_ENTRY
    let mut rsrc = Vec::with_capacity(ico.len() + 2048);

    // Level 1: Root (2 entries)
    write_rsrc_dir_header(&mut rsrc, 2);
    push_u32(&mut rsrc, 3); // RT_ICON
    let root_icon_ptr_pos = rsrc.len();
    push_u32(&mut rsrc, 0);
    push_u32(&mut rsrc, 14); // RT_GROUP_ICON
    let root_grp_ptr_pos = rsrc.len();
    push_u32(&mut rsrc, 0);

    // Level 2A: RT_ICON directory (count entries)
    let lvl2_icon_off = rsrc.len() as u32;
    patch_u32(&mut rsrc, root_icon_ptr_pos, 0x8000_0000 | lvl2_icon_off);
    write_rsrc_dir_header(&mut rsrc, count as u16);
    let mut lvl2_icon_ptrs = Vec::with_capacity(count);
    for i in 0..count {
        push_u32(&mut rsrc, (i + 1) as u32);
        lvl2_icon_ptrs.push(rsrc.len());
        push_u32(&mut rsrc, 0);
    }

    // Level 2B: RT_GROUP_ICON directory (1 entry: ID = 1)
    let lvl2_grp_off = rsrc.len() as u32;
    patch_u32(&mut rsrc, root_grp_ptr_pos, 0x8000_0000 | lvl2_grp_off);
    write_rsrc_dir_header(&mut rsrc, 1);
    push_u32(&mut rsrc, 1);
    let lvl2_grp_ptr_pos = rsrc.len();
    push_u32(&mut rsrc, 0);

    // Level 3: Language directories (0x0409) for each RT_ICON + RT_GROUP_ICON
    let mut data_entry_ptr_positions: Vec<usize> = Vec::with_capacity(count + 1);
    for ptr_pos in lvl2_icon_ptrs {
        let off = rsrc.len() as u32;
        patch_u32(&mut rsrc, ptr_pos, 0x8000_0000 | off);
        write_rsrc_dir_header(&mut rsrc, 1);
        push_u32(&mut rsrc, 0x0409); // en-US
        data_entry_ptr_positions.push(rsrc.len());
        push_u32(&mut rsrc, 0);
    }
    let lvl3_grp_off = rsrc.len() as u32;
    patch_u32(&mut rsrc, lvl2_grp_ptr_pos, 0x8000_0000 | lvl3_grp_off);
    write_rsrc_dir_header(&mut rsrc, 1);
    push_u32(&mut rsrc, 0x0409);
    data_entry_ptr_positions.push(rsrc.len());
    push_u32(&mut rsrc, 0);

    // IMAGE_RESOURCE_DATA_ENTRY array (16 bytes each)
    let mut reloc_offsets: Vec<u32> = Vec::with_capacity(count + 1);
    let mut data_rva_patch_positions: Vec<usize> = Vec::with_capacity(count + 1);
    for (idx, ptr_pos) in data_entry_ptr_positions.into_iter().enumerate() {
        let entry_off = rsrc.len() as u32;
        patch_u32(&mut rsrc, ptr_pos, entry_off); // Leaf entry (NO high bit!)
        reloc_offsets.push(entry_off); // Relocate OffsetToData field at entry_off + 0
        data_rva_patch_positions.push(rsrc.len());
        let blob_len = if idx < count {
            icon_blobs[idx].len() as u32
        } else {
            grp_blob.len() as u32
        };
        push_u32(&mut rsrc, 0);        // OffsetToData (patched to section-relative offset + ADDR32NB reloc)
        push_u32(&mut rsrc, blob_len); // Size
        push_u32(&mut rsrc, 0);        // CodePage
        push_u32(&mut rsrc, 0);        // Reserved
    }

    // Raw resource data blobs (4-byte aligned)
    for (idx, patch_pos) in data_rva_patch_positions.into_iter().enumerate() {
        pad4(&mut rsrc);
        let blob_off = rsrc.len() as u32;
        patch_u32(&mut rsrc, patch_pos, blob_off);
        if idx < count {
            rsrc.extend_from_slice(icon_blobs[idx]);
        } else {
            rsrc.extend_from_slice(&grp_blob);
        }
    }
    pad4(&mut rsrc);

    // Assemble standard x86_64 PE/COFF object file (.o)
    let raw_data_ptr: u32 = 20 + 40; // COFF header (20) + 1 section header (40)
    let reloc_ptr: u32 = raw_data_ptr + rsrc.len() as u32;
    let num_relocs = reloc_offsets.len() as u16;
    let symtab_ptr: u32 = reloc_ptr + (num_relocs as u32 * 10);

    let mut obj = Vec::with_capacity(symtab_ptr as usize + 64);

    // 1. COFF File Header (20 bytes)
    push_u16(&mut obj, 0x8664); // IMAGE_FILE_MACHINE_AMD64
    push_u16(&mut obj, 1);      // NumberOfSections
    push_u32(&mut obj, 0);      // TimeDateStamp
    push_u32(&mut obj, symtab_ptr);
    push_u32(&mut obj, 1);      // NumberOfSymbols (1 section symbol: .rsrc)
    push_u16(&mut obj, 0);      // SizeOfOptionalHeader
    push_u16(&mut obj, 0x0004); // IMAGE_FILE_LINE_NUMS_STRIPPED

    // 2. Section Header `.rsrc` (40 bytes)
    obj.extend_from_slice(b".rsrc\0\0\0");
    push_u32(&mut obj, 0);                  // VirtualSize
    push_u32(&mut obj, 0);                  // VirtualAddress
    push_u32(&mut obj, rsrc.len() as u32);  // SizeOfRawData
    push_u32(&mut obj, raw_data_ptr);       // PointerToRawData
    push_u32(&mut obj, reloc_ptr);          // PointerToRelocations
    push_u32(&mut obj, 0);                  // PointerToLinenumbers
    push_u16(&mut obj, num_relocs);         // NumberOfRelocations
    push_u16(&mut obj, 0);                  // NumberOfLinenumbers
    push_u32(&mut obj, 0x4000_0040);        // CNT_INITIALIZED_DATA | MEM_READ

    // 3. `.rsrc` Raw Section Data
    obj.extend_from_slice(&rsrc);

    // 4. Relocations (`IMAGE_REL_AMD64_ADDR32NB = 3` referencing symbol #0 `.rsrc`)
    for r_off in reloc_offsets {
        push_u32(&mut obj, r_off);
        push_u32(&mut obj, 0); // SymbolTableIndex = 0 (.rsrc)
        push_u16(&mut obj, 3); // IMAGE_REL_AMD64_ADDR32NB
    }

    // 5. Symbol Table (1 symbol: `.rsrc`, 18 bytes) + Empty String Table (4 bytes)
    obj.extend_from_slice(b".rsrc\0\0\0");
    push_u32(&mut obj, 0); // Value
    push_u16(&mut obj, 1); // SectionNumber = 1 (1-based)
    push_u16(&mut obj, 0); // Type
    obj.push(3);           // StorageClass = IMAGE_SYM_CLASS_STATIC
    obj.push(0);           // NumberOfAuxSymbols
    push_u32(&mut obj, 4); // String table total size = 4 bytes (empty)

    fs::write(obj_path, &obj).map_err(|e| format!("write {:?}: {}", obj_path, e))
}

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let icons_dir = manifest_dir.join("assets").join("icons");

    let bins = [
        "AuraShell",
        "AuraFiles",
        "AuraPhotos",
        "AuraPlayer",
        "AuraNotes",
        "AuraSettings",
        "AuraLock",
        "AuraTasks",
        "AuraPDF",
        "AuraShot",
        "AuraAgent",
    ];

    for bin in bins {
        let ico = icons_dir.join(format!("{}.ico", bin));
        println!("cargo:rerun-if-changed={}", ico.display());
        if ico.exists() {
            let obj_path = out_dir.join(format!("{}_icon.o", bin));
            if let Err(e) = build_ico_coff(&ico, &obj_path) {
                println!("cargo:warning=Icon resource skipped for {}: {}", bin, e);
            } else {
                println!("cargo:rustc-link-arg-bin={}={}", bin, obj_path.display());
            }
        }
    }
}
