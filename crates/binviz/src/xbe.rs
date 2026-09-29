//! Original Xbox executables (XBE): the `default.xbe` a game disc boots, and
//! the other `.xbe` files beside it. The XDK's image builder makes one from
//! the PE its linker wrote, so what is inside is 32-bit x86 code and data at
//! fixed addresses (an XBE has no base relocations), in a container of its own:
//!
//! - the image header: where the image is loaded (the base address, usually
//!   `0x10000`), the entry point and the kernel thunk table's address (each
//!   XOR-ed with a key that differs between retail and development consoles),
//!   the TLS directory, what the PE said (stack and heap sizes, base, size,
//!   checksum, timestamp), and where the rest of the headers are;
//! - the certificate: the title's ID and name, alternate title IDs, the media
//!   it may run from, its regions, ratings, disk number, version and keys;
//! - the section headers (flags, where each section is in memory and in the
//!   file, its name, the counters of the pages it shares with its
//!   neighbours, a SHA-1 digest) and the names they point to;
//! - the versions of the XDK libraries linked in (`XAPILIB 1.0.5849`), which
//!   say which XDK built the game;
//! - the file names the PE was built under, and the Microsoft logo.
//!
//! The headers are mapped at the base address as they are in the file, so
//! an address the header holds for something in the headers is the base
//! address plus that thing's file offset. The kernel is called through the
//! kernel thunk table: its slots hold the ordinals of `xboxkrnl.exe`'s
//! exports (with the high bit set) until the loader writes the exports'
//! addresses over them, so `call [slot]` is a kernel call, named after the
//! ordinal (see [`KERNEL_EXPORTS`]).
//!
//! The offsets are those the XboxDev wiki's "Xbe" page and Cxbx-Reloaded's
//! `Xbe.h` give. The fields past them that later XDKs write are read only
//! where the headers say they are there, and their doubts are noted where
//! they are laid out (`layout/xbe.rs`).

use std::sync::Arc;

use crate::binary::Binary;
use crate::discover::x86;
use crate::error::{Error, Result};
use crate::layout::{Layout, Machine};
use crate::model::{Format, Import, Property, RegionKind, Section, Segment, Summary, SymbolKind, SymbolSource};
use crate::symbols::{Binding, Builder as SymbolBuilder, NewSym};
use crate::util::{self, Bytes, Endian};

/// The size of the image header every XBE has: up to the logo's size, at 0x174.
pub(crate) const IMAGE_HEADER_SIZE: u64 = 0x178;
/// The size of a section header.
pub(crate) const SECTION_HEADER_SIZE: u64 = 0x38;
/// The size of a library version.
pub(crate) const LIBRARY_VERSION_SIZE: u64 = 16;
/// The library the kernel's exports come from.
pub(crate) const KERNEL: &str = "xboxkrnl.exe";

/// The image header's fields binviz reads, at the offsets noted.
pub(crate) struct Header {
    /// 0x104: where the headers are mapped, and the image with them.
    pub base: u64,
    /// 0x108: the bytes of headers mapped at the base address.
    pub size_of_headers: u64,
    /// 0x10C: from the base address to the end of the last section.
    pub size_of_image: u64,
    /// 0x110: 0x178, or more in XBEs later XDKs built.
    pub size_of_image_header: u64,
    /// 0x114: seconds since 1970.
    pub timestamp: u64,
    /// 0x118
    pub certificate: u64,
    /// 0x11C
    pub section_count: u64,
    /// 0x120
    pub section_headers: u64,
    /// 0x124
    pub init_flags: u32,
    /// 0x128: the entry point XOR-ed with a key (see [`Build`]).
    pub entry_encoded: u32,
    /// 0x12C: the TLS directory, in a section.
    pub tls: u64,
    // 0x130 to 0x148: the PE's stack commit, heap reserve and commit, base
    // address, size of image, checksum and timestamp.
    /// 0x14C: the path the PE was built under.
    pub debug_path: u64,
    /// 0x150: its file name, usually inside the path's string.
    pub debug_file: u64,
    /// 0x154: the file name again, in UTF-16.
    pub debug_unicode_file: u64,
    /// 0x158: the kernel thunk table's address XOR-ed with a key (see [`Build`]).
    pub thunks_encoded: u32,
    /// 0x15C: imports from other executables than the kernel (zero in games).
    pub import_directory: u64,
    /// 0x160
    pub library_count: u64,
    /// 0x164
    pub libraries: u64,
    // 0x168: the kernel's entry among the library versions (XBOXKRNL).
    /// 0x16C: XAPILIB's entry among the library versions.
    pub xapi_library: u64,
    /// 0x170
    pub logo: u64,
    /// 0x174
    pub logo_size: u64,
}

impl Header {
    /// The image header at the start of `data`, if it is an XBE's (by its magic, `XBEH`).
    pub(crate) fn read(data: &[u8]) -> Option<Header> {
        if data.get(..4)? != b"XBEH" || (data.len() as u64) < IMAGE_HEADER_SIZE {
            return None;
        }
        let b = Bytes::new(data, Endian::Little);
        let word = |at: u64| b.u32(at).map_or(0, u64::from);
        Some(Header {
            base: word(0x104),
            size_of_headers: word(0x108),
            size_of_image: word(0x10C),
            size_of_image_header: word(0x110),
            timestamp: word(0x114),
            certificate: word(0x118),
            section_count: word(0x11C),
            section_headers: word(0x120),
            init_flags: word(0x124) as u32,
            entry_encoded: word(0x128) as u32,
            tls: word(0x12C),
            debug_path: word(0x14C),
            debug_file: word(0x150),
            debug_unicode_file: word(0x154),
            thunks_encoded: word(0x158) as u32,
            import_directory: word(0x15C),
            library_count: word(0x160),
            libraries: word(0x164),
            xapi_library: word(0x16C),
            logo: word(0x170),
            logo_size: word(0x174),
        })
    }

    /// The file offset of an address in the headers.
    pub(crate) fn offset(&self, address: u64) -> Option<u64> {
        address.checked_sub(self.base).filter(|&o| o < self.size_of_headers)
    }

    /// Whether an address is in the image.
    fn inside(&self, address: u64) -> bool {
        address >= self.base && address - self.base < self.size_of_image
    }

    /// Which consoles the XBE is for, and the entry point and kernel thunk
    /// table's address decoded with their keys: the keys that put both
    /// inside the image. The keys differ in their high bits, so no two
    /// builds' keys can both do that.
    pub(crate) fn keys(&self) -> Option<(Build, u64, u64)> {
        Build::ALL.into_iter().find_map(|build| {
            let entry = (self.entry_encoded ^ build.entry_key()) as u64;
            let thunks = (self.thunks_encoded ^ build.thunk_key()) as u64;
            (self.inside(entry) && self.inside(thunks)).then_some((build, entry, thunks))
        })
    }
}

/// Whether `data` is an XBE: its magic, and an image header whose sizes make sense.
pub(crate) fn is_xbe(data: &[u8]) -> bool {
    Header::read(data).is_some_and(|h| {
        h.base != 0 && h.size_of_image_header >= IMAGE_HEADER_SIZE && h.size_of_headers >= h.size_of_image_header
    })
}

/// The consoles an XBE is for, told apart by the keys its entry point and
/// kernel thunk table address are XOR-ed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Build {
    Retail,
    /// Development kits.
    Debug,
    /// Sega's Chihiro arcade board, which runs XBEs too (its keys as
    /// Cxbx-Reloaded gives them; tried last).
    Chihiro,
}

impl Build {
    const ALL: [Build; 3] = [Build::Retail, Build::Debug, Build::Chihiro];

    pub(crate) fn entry_key(self) -> u32 {
        match self {
            Build::Retail => 0xA8FC_57AB,
            Build::Debug => 0x9485_9D4B,
            Build::Chihiro => 0x40B5_C16E,
        }
    }

    pub(crate) fn thunk_key(self) -> u32 {
        match self {
            Build::Retail => 0x5B6D_40B6,
            Build::Debug => 0xEFB1_F152,
            Build::Chihiro => 0x2290_059D,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Build::Retail => "retail",
            Build::Debug => "debug",
            Build::Chihiro => "Chihiro",
        }
    }
}

/// Section flags.
pub(crate) const WRITABLE: u32 = 0x01;
pub(crate) const PRELOAD: u32 = 0x02;
pub(crate) const EXECUTABLE: u32 = 0x04;
/// Data the image builder inserted from a file (the title image, `$$XTIMAGE`).
pub(crate) const INSERTED_FILE: u32 = 0x08;
pub(crate) const HEAD_PAGE_READ_ONLY: u32 = 0x10;
pub(crate) const TAIL_PAGE_READ_ONLY: u32 = 0x20;

const SECTION_FLAGS: [(u32, &str); 6] = [
    (WRITABLE, "writable"),
    (PRELOAD, "preload"),
    (EXECUTABLE, "executable"),
    (INSERTED_FILE, "inserted file"),
    (HEAD_PAGE_READ_ONLY, "head page read-only"),
    (TAIL_PAGE_READ_ONLY, "tail page read-only"),
];

/// The image header's initialization flags; other bits are shown as numbers.
const INIT_FLAGS: [(u32, &str); 4] = [
    (0x1, "mount utility drive"),
    (0x2, "format utility drive"),
    (0x4, "limit development kit memory to 64 MiB"),
    (0x8, "don't set up hard disk"),
];

/// The media the certificate allows the game to run from.
const MEDIA: [(u32, &str); 12] = [
    (0x0000_0001, "hard disk"),
    (0x0000_0002, "DVD-X2"),
    (0x0000_0004, "DVD / CD"),
    (0x0000_0008, "CD"),
    (0x0000_0010, "DVD-5 read-only"),
    (0x0000_0020, "DVD-9 read-only"),
    (0x0000_0040, "DVD-5 rewritable"),
    (0x0000_0080, "DVD-9 rewritable"),
    (0x0000_0100, "dongle"),
    (0x0000_0200, "media board"),
    (0x4000_0000, "non-secure hard disk"),
    (0x8000_0000, "non-secure mode"),
];

/// The regions the certificate allows the game to run in.
const REGIONS: [(u32, &str); 4] = [
    (0x0000_0001, "North America"),
    (0x0000_0002, "Japan"),
    (0x0000_0004, "rest of the world"),
    (0x8000_0000, "manufacturing"),
];

/// The names of the bits set in `v`, and what is left as a number.
fn flag_names(names: &[(u32, &str)], v: u32) -> String {
    let mut out: Vec<String> = names
        .iter()
        .filter(|(bit, _)| v & bit != 0)
        .map(|(_, n)| n.to_string())
        .collect();
    let rest = names.iter().fold(v, |v, (bit, _)| v & !bit);
    if rest != 0 {
        out.push(format!("{rest:#x}"));
    }
    out.join(", ")
}

pub(crate) fn section_flags(v: u64) -> String {
    flag_names(&SECTION_FLAGS, v as u32)
}

pub(crate) fn init_flags(v: u64) -> String {
    flag_names(&INIT_FLAGS, v as u32)
}

pub(crate) fn media(v: u64) -> String {
    flag_names(&MEDIA, v as u32)
}

pub(crate) fn regions(v: u64) -> String {
    flag_names(&REGIONS, v as u32)
}

/// A section header (the reference count at 0x18 and the digest at 0x24 are
/// only laid out).
pub(crate) struct SectionHeader {
    /// 0x00
    pub flags: u32,
    /// 0x04
    pub address: u64,
    /// 0x08
    pub size: u64,
    /// 0x0C: the file offset of its bytes.
    pub raw_address: u64,
    /// 0x10
    pub raw_size: u64,
    /// 0x14: the address of its name, in the headers.
    pub name_address: u64,
    /// 0x1C and 0x20: the counters (16-bit, in the headers) of the pages it
    /// shares with the sections before and after it.
    pub head_page: u64,
    pub tail_page: u64,
    pub name: String,
}

impl SectionHeader {
    /// What the section holds, by its flags.
    pub(crate) fn kind(&self) -> RegionKind {
        if self.flags & EXECUTABLE != 0 {
            RegionKind::Code
        } else if self.flags & INSERTED_FILE != 0 {
            RegionKind::Resources
        } else if self.flags & WRITABLE != 0 {
            if self.raw_size == 0 {
                RegionKind::Bss
            } else {
                RegionKind::Data
            }
        } else {
            RegionKind::Rodata
        }
    }
}

/// The section headers, with their names.
pub(crate) fn section_headers(b: &Bytes, h: &Header) -> Vec<SectionHeader> {
    let Some(table) = h.offset(h.section_headers) else {
        return Vec::new();
    };
    (0..h.section_count.min(4096))
        .map_while(|i| {
            let at = table + i * SECTION_HEADER_SIZE;
            let word = |o: u64| b.u32(at + o).map(u64::from);
            let name_address = word(0x14)?;
            Some(SectionHeader {
                flags: word(0x00)? as u32,
                address: word(0x04)?,
                size: word(0x08)?,
                raw_address: word(0x0C)?,
                raw_size: word(0x10)?,
                name_address,
                head_page: word(0x1C)?,
                tail_page: word(0x20)?,
                name: h
                    .offset(name_address)
                    .and_then(|o| b.cstr(o, 64))
                    .map(util::lossy)
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// The file offset of an address in a section's bytes.
pub(crate) fn section_offset(sections: &[Section], address: u64) -> Option<u64> {
    sections.iter().find_map(|s| {
        let o = s.file_offset?;
        (address >= s.address && address - s.address < s.file_size.min(s.size)).then(|| o + (address - s.address))
    })
}

/// The kernel thunk table at `address`: each slot's address and the export
/// ordinal it holds, up to the zero that ends the table (or a word without
/// the ordinal bit, which no kernel import is).
pub(crate) fn kernel_thunks(b: &Bytes, sections: &[Section], address: u64) -> Vec<(u64, u32)> {
    let mut out = Vec::new();
    // The kernel exports fewer than 400 functions and variables.
    for i in 0..1024 {
        let slot = address + 4 * i;
        match section_offset(sections, slot).and_then(|o| b.u32(o)) {
            Some(v) if v & 0x8000_0000 != 0 => out.push((slot, v & 0x7FFF_FFFF)),
            _ => break,
        }
    }
    out
}

/// The functions the TLS directory lists (its `AddressOfCallBacks`, at 12, as
/// in a PE's), which the loader calls as threads start and end.
pub(crate) fn tls_callbacks(b: &Bytes, sections: &[Section], tls: u64) -> Vec<u64> {
    let read = |address: u64| section_offset(sections, address).and_then(|o| b.u32(o)).map(u64::from);
    let Some(list) = (tls != 0).then(|| read(tls + 12)).flatten().filter(|&l| l != 0) else {
        return Vec::new();
    };
    (0..256).map_while(|i| read(list + 4 * i).filter(|&f| f != 0)).collect()
}

/// A library version: an XDK library the game linked, and its build.
#[derive(Clone)]
pub(crate) struct Library {
    pub name: String,
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    /// The QFE (hotfix) number in bits 0–12, whether Microsoft approved the
    /// library in bits 13–14 (0 no, 1 possibly, 2 yes), a debug build in bit 15.
    pub flags: u16,
}

impl Library {
    pub(crate) fn read(b: &Bytes, at: u64) -> Option<Library> {
        Some(Library {
            name: util::fixed_str(b.slice(at, 8)?),
            major: b.u16(at + 8)?,
            minor: b.u16(at + 10)?,
            build: b.u16(at + 12)?,
            flags: b.u16(at + 14)?,
        })
    }

    /// `XAPILIB 1.0.5849`, with its QFE and whether it is a debug build.
    pub(crate) fn describe(&self) -> String {
        let mut text = format!("{} {}.{}.{}", self.name, self.major, self.minor, self.build);
        match (self.flags & 0x1FFF, self.flags & 0x8000 != 0) {
            (0, false) => {}
            (0, true) => text.push_str(" (debug)"),
            (qfe, false) => text.push_str(&format!(" (QFE {qfe})")),
            (qfe, true) => text.push_str(&format!(" (QFE {qfe}, debug)")),
        }
        text
    }
}

/// The QFE number, approval and debug bit of a library version's flags.
pub(crate) fn library_flags(v: u64) -> String {
    let approval = match (v >> 13) & 3 {
        0 => "not approved",
        1 => "possibly approved",
        2 => "approved",
        _ => "approval 3",
    };
    let debug = if v & 0x8000 != 0 { ", debug build" } else { "" };
    format!("QFE {}, {approval}{debug} ({v:#06x})", v & 0x1FFF)
}

/// The library versions.
pub(crate) fn libraries(b: &Bytes, h: &Header) -> Vec<Library> {
    let Some(at) = h.offset(h.libraries) else {
        return Vec::new();
    };
    (0..h.library_count.min(1024))
        .map_while(|i| Library::read(b, at + i * LIBRARY_VERSION_SIZE))
        .collect()
}

/// UTF-16 text up to its terminating zero.
pub(crate) fn utf16(bytes: &[u8]) -> String {
    let units = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0);
    char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// A title ID as it is written: `4D530004 (MS-004)`, its high half the
/// publisher's two letters and its low half the game's number.
pub(crate) fn title_id(id: u32) -> String {
    let (a, b) = ((id >> 24) as u8, (id >> 16) as u8);
    if a.is_ascii_graphic() && b.is_ascii_graphic() {
        format!("{id:08X} ({}{}-{:03})", a as char, b as char, id & 0xFFFF)
    } else {
        format!("{id:08X}")
    }
}

/// The names of `xboxkrnl.exe`'s exports, by ordinal: the first is ordinal 1.
///
/// Games import the kernel by ordinal only, so the names come from outside
/// the file. These are the ordinals the open-source Xbox toolchain and
/// emulator use (nxdk's `lib/xboxkrnl/xboxkrnl.exe.def`, Cxbx-Reloaded's
/// kernel thunk table), transcribed without a copy of either at hand: check
/// a surprising name against nxdk's file. They hold up to two checks: the
/// first 328 run in alphabetical order, prefix by prefix (Av, Dbg, Ex, Fsc,
/// Hal, Interlocked, Io, Kd, Ke, Mm, Nt, Ob, Phy, Ps, Rtl, Xbox, Xe), but for
/// a few numbered out of turn (`HalReadSMCTrayState` 9,
/// `ExQueryNonVolatileSetting` and `ExReadWriteRefurbInfo` 24 and 25,
/// `IoDismountVolume…` 90 and 91, `MmGlobalData` 102, and a few pairs inside
/// a run); and the ordinals decompilations and emulator logs cite on their
/// own (`DbgPrint` 8, `HalReturnToFirmware` 49, `KeBugCheck` 95,
/// `KeTickCount` 156, `MmAllocateContiguousMemory` 165, `NtClose` 187,
/// `NtCreateFile` 190, `PsTerminateSystemThread` 258, `RtlInitAnsiString`
/// 289, `XeLoadSection` 327) fall where the runs put them. Ordinals from 367
/// on (development kits' `MmDbg…` functions, and exports no retail kernel
/// has) are left unnamed rather than guessed.
#[rustfmt::skip]
const KERNEL_EXPORTS: [&str; 366] = [
    /*   1 */ "AvGetSavedDataAddress", "AvSendTVEncoderOption", "AvSetDisplayMode", "AvSetSavedDataAddress",
    /*   5 */ "DbgBreakPoint", "DbgBreakPointWithStatus", "DbgLoadImageSymbols", "DbgPrint",
    /*   9 */ "HalReadSMCTrayState", "DbgPrompt", "DbgUnLoadImageSymbols", "ExAcquireReadWriteLockExclusive",
    /*  13 */ "ExAcquireReadWriteLockShared", "ExAllocatePool", "ExAllocatePoolWithTag", "ExEventObjectType",
    /*  17 */ "ExFreePool", "ExInitializeReadWriteLock", "ExInterlockedAddLargeInteger",
              "ExInterlockedAddLargeStatistic",
    /*  21 */ "ExInterlockedCompareExchange64", "ExMutantObjectType", "ExQueryPoolBlockSize",
              "ExQueryNonVolatileSetting",
    /*  25 */ "ExReadWriteRefurbInfo", "ExRaiseException", "ExRaiseStatus", "ExReleaseReadWriteLock",
    /*  29 */ "ExSaveNonVolatileSetting", "ExSemaphoreObjectType", "ExTimerObjectType",
              "ExfInterlockedInsertHeadList",
    /*  33 */ "ExfInterlockedInsertTailList", "ExfInterlockedRemoveHeadList", "FscGetCacheSize",
              "FscInvalidateIdleBlocks",
    /*  37 */ "FscSetCacheSize", "HalClearSoftwareInterrupt", "HalDisableSystemInterrupt",
              "HalDiskCachePartitionCount",
    /*  41 */ "HalDiskModelNumber", "HalDiskSerialNumber", "HalEnableSystemInterrupt", "HalGetInterruptVector",
    /*  45 */ "HalReadSMBusValue", "HalReadWritePCISpace", "HalRegisterShutdownNotification",
              "HalRequestSoftwareInterrupt",
    /*  49 */ "HalReturnToFirmware", "HalWriteSMBusValue", "InterlockedCompareExchange", "InterlockedDecrement",
    /*  53 */ "InterlockedIncrement", "InterlockedExchange", "InterlockedExchangeAdd", "InterlockedFlushSList",
    /*  57 */ "InterlockedPopEntrySList", "InterlockedPushEntrySList", "IoAllocateIrp",
              "IoBuildAsynchronousFsdRequest",
    /*  61 */ "IoBuildDeviceIoControlRequest", "IoBuildSynchronousFsdRequest", "IoCheckShareAccess",
              "IoCompletionObjectType",
    /*  65 */ "IoCreateDevice", "IoCreateFile", "IoCreateSymbolicLink", "IoDeleteDevice",
    /*  69 */ "IoDeleteSymbolicLink", "IoDeviceObjectType", "IoFileObjectType", "IoFreeIrp",
    /*  73 */ "IoInitializeIrp", "IoInvalidDeviceRequest", "IoQueryFileInformation", "IoQueryVolumeInformation",
    /*  77 */ "IoQueueThreadIrp", "IoRemoveShareAccess", "IoSetIoCompletion", "IoSetShareAccess",
    /*  81 */ "IoStartNextPacket", "IoStartNextPacketByKey", "IoStartPacket", "IoSynchronousDeviceIoControlRequest",
    /*  85 */ "IoSynchronousFsdRequest", "IofCallDriver", "IofCompleteRequest", "KdDebuggerEnabled",
    /*  89 */ "KdDebuggerNotPresent", "IoDismountVolume", "IoDismountVolumeByName", "KeAlertResumeThread",
    /*  93 */ "KeAlertThread", "KeBoostPriorityThread", "KeBugCheck", "KeBugCheckEx",
    /*  97 */ "KeCancelTimer", "KeConnectInterrupt", "KeDelayExecutionThread", "KeDisconnectInterrupt",
    /* 101 */ "KeEnterCriticalRegion", "MmGlobalData", "KeGetCurrentIrql", "KeGetCurrentThread",
    /* 105 */ "KeInitializeApc", "KeInitializeDeviceQueue", "KeInitializeDpc", "KeInitializeEvent",
    /* 109 */ "KeInitializeInterrupt", "KeInitializeMutant", "KeInitializeQueue", "KeInitializeSemaphore",
    /* 113 */ "KeInitializeTimerEx", "KeInsertByKeyDeviceQueue", "KeInsertDeviceQueue", "KeInsertHeadQueue",
    /* 117 */ "KeInsertQueue", "KeInsertQueueApc", "KeInsertQueueDpc", "KeInterruptTime",
    /* 121 */ "KeIsExecutingDpc", "KeLeaveCriticalRegion", "KePulseEvent", "KeQueryBasePriorityThread",
    /* 125 */ "KeQueryInterruptTime", "KeQueryPerformanceCounter", "KeQueryPerformanceFrequency",
              "KeQuerySystemTime",
    /* 129 */ "KeRaiseIrqlToDpcLevel", "KeRaiseIrqlToSynchLevel", "KeReleaseMutant", "KeReleaseSemaphore",
    /* 133 */ "KeRemoveByKeyDeviceQueue", "KeRemoveDeviceQueue", "KeRemoveEntryDeviceQueue", "KeRemoveQueue",
    /* 137 */ "KeRemoveQueueDpc", "KeResetEvent", "KeRestoreFloatingPointState", "KeResumeThread",
    /* 141 */ "KeRundownQueue", "KeSaveFloatingPointState", "KeSetBasePriorityThread", "KeSetDisableBoostThread",
    /* 145 */ "KeSetEvent", "KeSetEventBoostPriority", "KeSetPriorityProcess", "KeSetPriorityThread",
    /* 149 */ "KeSetTimer", "KeSetTimerEx", "KeStallExecutionProcessor", "KeSuspendThread",
    /* 153 */ "KeSynchronizeExecution", "KeSystemTime", "KeTestAlertThread", "KeTickCount",
    /* 157 */ "KeTimeIncrement", "KeWaitForMultipleObjects", "KeWaitForSingleObject", "KfRaiseIrql",
    /* 161 */ "KfLowerIrql", "KiBugCheckData", "KiUnlockDispatcherDatabase", "LaunchDataPage",
    /* 165 */ "MmAllocateContiguousMemory", "MmAllocateContiguousMemoryEx", "MmAllocateSystemMemory",
              "MmClaimGpuInstanceMemory",
    /* 169 */ "MmCreateKernelStack", "MmDeleteKernelStack", "MmFreeContiguousMemory", "MmFreeSystemMemory",
    /* 173 */ "MmGetPhysicalAddress", "MmIsAddressValid", "MmLockUnlockBufferPages", "MmLockUnlockPhysicalPage",
    /* 177 */ "MmMapIoSpace", "MmPersistContiguousMemory", "MmQueryAddressProtect", "MmQueryAllocationSize",
    /* 181 */ "MmQueryStatistics", "MmSetAddressProtect", "MmUnmapIoSpace", "NtAllocateVirtualMemory",
    /* 185 */ "NtCancelTimer", "NtClearEvent", "NtClose", "NtCreateDirectoryObject",
    /* 189 */ "NtCreateEvent", "NtCreateFile", "NtCreateIoCompletion", "NtCreateMutant",
    /* 193 */ "NtCreateSemaphore", "NtCreateTimer", "NtDeleteFile", "NtDeviceIoControlFile",
    /* 197 */ "NtDuplicateObject", "NtFlushBuffersFile", "NtFreeVirtualMemory", "NtFsControlFile",
    /* 201 */ "NtOpenDirectoryObject", "NtOpenFile", "NtOpenSymbolicLinkObject", "NtProtectVirtualMemory",
    /* 205 */ "NtPulseEvent", "NtQueueApcThread", "NtQueryDirectoryFile", "NtQueryDirectoryObject",
    /* 209 */ "NtQueryEvent", "NtQueryFullAttributesFile", "NtQueryInformationFile", "NtQueryIoCompletion",
    /* 213 */ "NtQueryMutant", "NtQuerySemaphore", "NtQuerySymbolicLinkObject", "NtQueryTimer",
    /* 217 */ "NtQueryVirtualMemory", "NtQueryVolumeInformationFile", "NtReadFile", "NtReadFileScatter",
    /* 221 */ "NtReleaseMutant", "NtReleaseSemaphore", "NtRemoveIoCompletion", "NtResumeThread",
    /* 225 */ "NtSetEvent", "NtSetInformationFile", "NtSetIoCompletion", "NtSetSystemTime",
    /* 229 */ "NtSetTimerEx", "NtSignalAndWaitForSingleObjectEx", "NtSuspendThread", "NtUserIoApcDispatcher",
    /* 233 */ "NtWaitForSingleObject", "NtWaitForSingleObjectEx", "NtWaitForMultipleObjectsEx", "NtWriteFile",
    /* 237 */ "NtWriteFileGather", "NtYieldExecution", "ObCreateObject", "ObDirectoryObjectType",
    /* 241 */ "ObInsertObject", "ObMakeTemporaryObject", "ObOpenObjectByName", "ObOpenObjectByPointer",
    /* 245 */ "ObpObjectHandleTable", "ObReferenceObjectByHandle", "ObReferenceObjectByName",
              "ObReferenceObjectByPointer",
    /* 249 */ "ObSymbolicLinkObjectType", "ObfDereferenceObject", "ObfReferenceObject", "PhyGetLinkState",
    /* 253 */ "PhyInitialize", "PsCreateSystemThread", "PsCreateSystemThreadEx", "PsQueryStatistics",
    /* 257 */ "PsSetCreateThreadNotifyRoutine", "PsTerminateSystemThread", "PsThreadObjectType",
              "RtlAnsiStringToUnicodeString",
    /* 261 */ "RtlAppendStringToString", "RtlAppendUnicodeStringToString", "RtlAppendUnicodeToString",
              "RtlAssert",
    /* 265 */ "RtlCaptureContext", "RtlCaptureStackBackTrace", "RtlCharToInteger", "RtlCompareMemory",
    /* 269 */ "RtlCompareMemoryUlong", "RtlCompareString", "RtlCompareUnicodeString", "RtlCopyString",
    /* 273 */ "RtlCopyUnicodeString", "RtlCreateUnicodeString", "RtlDowncaseUnicodeChar",
              "RtlDowncaseUnicodeString",
    /* 277 */ "RtlEnterCriticalSection", "RtlEnterCriticalSectionAndRegion", "RtlEqualString",
              "RtlEqualUnicodeString",
    /* 281 */ "RtlExtendedIntegerMultiply", "RtlExtendedLargeIntegerDivide", "RtlExtendedMagicDivide",
              "RtlFillMemory",
    /* 285 */ "RtlFillMemoryUlong", "RtlFreeAnsiString", "RtlFreeUnicodeString", "RtlGetCallersAddress",
    /* 289 */ "RtlInitAnsiString", "RtlInitUnicodeString", "RtlInitializeCriticalSection", "RtlIntegerToChar",
    /* 293 */ "RtlIntegerToUnicodeString", "RtlLeaveCriticalSection", "RtlLeaveCriticalSectionAndRegion",
              "RtlLowerChar",
    /* 297 */ "RtlMapGenericMask", "RtlMoveMemory", "RtlMultiByteToUnicodeN", "RtlMultiByteToUnicodeSize",
    /* 301 */ "RtlNtStatusToDosError", "RtlRaiseException", "RtlRaiseStatus", "RtlTimeFieldsToTime",
    /* 305 */ "RtlTimeToTimeFields", "RtlTryEnterCriticalSection", "RtlUlongByteSwap",
              "RtlUnicodeStringToAnsiString",
    /* 309 */ "RtlUnicodeStringToInteger", "RtlUnicodeToMultiByteN", "RtlUnicodeToMultiByteSize", "RtlUnwind",
    /* 313 */ "RtlUpcaseUnicodeChar", "RtlUpcaseUnicodeString", "RtlUpcaseUnicodeToMultiByteN", "RtlUpperChar",
    /* 317 */ "RtlUpperString", "RtlUshortByteSwap", "RtlWalkFrameChain", "RtlZeroMemory",
    /* 321 */ "XboxEEPROMKey", "XboxHardwareInfo", "XboxHDKey", "XboxKrnlVersion",
    /* 325 */ "XboxSignatureKey", "XeImageFileName", "XeLoadSection", "XeUnloadSection",
    /* 329 */ "READ_PORT_BUFFER_UCHAR", "READ_PORT_BUFFER_USHORT", "READ_PORT_BUFFER_ULONG",
              "WRITE_PORT_BUFFER_UCHAR",
    /* 333 */ "WRITE_PORT_BUFFER_USHORT", "WRITE_PORT_BUFFER_ULONG", "XcSHAInit", "XcSHAUpdate",
    /* 337 */ "XcSHAFinal", "XcRC4Key", "XcRC4Crypt", "XcHMAC",
    /* 341 */ "XcPKEncPublic", "XcPKDecPrivate", "XcPKGetKeyLen", "XcVerifyPKCS1Signature",
    /* 345 */ "XcModExp", "XcDESKeyParity", "XcKeyTable", "XcBlockCrypt",
    /* 349 */ "XcBlockCryptCBC", "XcCryptService", "XcUpdateCrypto", "RtlRip",
    /* 353 */ "XboxLANKey", "XboxAlternateSignatureKeys", "XePublicKeyData", "HalBootSMCVideoMode",
    /* 357 */ "IdexChannelObject", "HalIsResetOrShutdownPending", "IoMarkIrpMustComplete", "HalInitiateShutdown",
    /* 361 */ "RtlSnprintf", "RtlSprintf", "RtlVsnprintf", "RtlVsprintf",
    /* 365 */ "HalEnableSecureTrayEject", "HalWriteSMCScratchRegister",
];

/// The kernel export an ordinal names, when [`KERNEL_EXPORTS`] knows it.
pub(crate) fn kernel_export(ordinal: u32) -> Option<&'static str> {
    KERNEL_EXPORTS.get((ordinal as usize).checked_sub(1)?).copied()
}

/// The name a kernel import goes by: its export's, else `xboxkrnl.exe #N`.
pub(crate) fn kernel_import_name(ordinal: u32) -> String {
    kernel_export(ordinal).map_or_else(|| format!("{KERNEL} #{ordinal}"), str::to_string)
}

/// Whether the kernel export an ordinal names never returns:
/// `HalReturnToFirmware` (49) reboots, shuts down or starts another
/// executable, `KeBugCheck` (95) and `KeBugCheckEx` (96) stop the system,
/// `PsTerminateSystemThread` (258) ends the calling thread. The exports that
/// raise exceptions (`ExRaiseStatus`, `RtlRaiseStatus`) are left out: a
/// handler may take over, and the code after them is taken to run.
fn never_returns(ordinal: u32) -> bool {
    matches!(ordinal, 49 | 95 | 96 | 258)
}

impl Binary {
    /// An XBE read into the model: its sections at their addresses, the
    /// kernel imports named in the thunk table's slots, and the code
    /// followed from the entry point and TLS callbacks.
    pub(crate) fn from_xbe(data: Arc<[u8]>) -> Result<Binary> {
        let bytes: &[u8] = &data;
        let len = bytes.len() as u64;
        let b = Bytes::new(bytes, Endian::Little);
        let h = Header::read(bytes).ok_or_else(|| Error::new("not an XBE: no XBEH image header"))?;

        // Sections, and a segment for each besides the headers', as the loader maps them.
        let headers = section_headers(&b, &h);
        let mut segments = vec![Segment {
            index: 0,
            name: "Headers".into(),
            kind: "Headers".into(),
            address: h.base,
            mem_size: h.size_of_headers,
            file_offset: 0,
            file_size: h.size_of_headers.min(len),
            align: 0,
            perms: "r--".into(),
            mapped: true,
        }];
        let mut sections = Vec::new();
        for s in &headers {
            let file_offset = (s.raw_size > 0 && s.raw_address < len).then_some(s.raw_address);
            let file_size = file_offset.map_or(0, |o| s.raw_size.min(len - o));
            let perms = format!(
                "r{}{}",
                if s.flags & WRITABLE != 0 { 'w' } else { '-' },
                if s.flags & EXECUTABLE != 0 { 'x' } else { '-' }
            );
            let index = sections.len() as u32;
            let name = if s.name.is_empty() {
                format!("(section {index})")
            } else {
                s.name.clone()
            };
            segments.push(Segment {
                index: segments.len() as u32,
                name: name.clone(),
                kind: "Section".into(),
                address: s.address,
                mem_size: s.size,
                file_offset: file_offset.unwrap_or(0),
                file_size: file_size.min(s.size),
                align: 0,
                perms: perms.clone(),
                mapped: true,
            });
            sections.push(Section {
                index,
                name,
                segment_name: None,
                kind: s.kind(),
                address: s.address,
                size: s.size,
                file_offset,
                file_size,
                align: 0,
                flags: section_flags(s.flags as u64),
                perms,
                compressed: false,
                segment: Some(segments.len() as u32 - 1),
                loaded: true,
            });
        }
        let section_of = |a: u64| {
            sections
                .iter()
                .find(|s| a >= s.address && a - s.address < s.size.max(1))
                .map(|s| s.index)
        };

        // The keys say where the entry point and the kernel thunk table are.
        let keys = h.keys();
        let entry = keys.map(|k| k.1);
        let thunks = keys.map_or_else(Vec::new, |k| kernel_thunks(&b, &sections, k.2));
        let imports: Vec<Import> = thunks
            .iter()
            .map(|&(slot, ordinal)| Import {
                library: KERNEL.into(),
                name: kernel_import_name(ordinal),
                demangled: None,
                ordinal: Some(ordinal),
                address: Some(slot),
            })
            .collect();
        let mut symbols = SymbolBuilder::default();
        for imp in &imports {
            let slot = imp.address.unwrap_or(0);
            symbols.push(NewSym {
                name: &format!("__imp_{}", imp.name),
                address: slot,
                size: 4,
                kind: SymbolKind::Data,
                binding: Binding::Global,
                section: section_of(slot),
                source: SymbolSource::Import,
                defined: true,
                plain: true,
            });
        }
        let symbols = symbols.finish_unindexed();

        // The code, followed as in a 32-bit PE linked without base relocations.
        let mut regions: Vec<x86::Region<'_>> = sections
            .iter()
            .filter_map(|s| {
                Some(x86::Region {
                    address: s.address,
                    bytes: b.slice(s.file_offset?, s.file_size.min(s.size))?,
                    code: s.kind == RegionKind::Code,
                })
            })
            .filter(|r| !r.bytes.is_empty())
            .collect();
        regions.sort_by_key(|r| r.address);
        // Sections don't overlap in a sound XBE; in one that isn't, the first of two wins.
        let mut end = 0;
        regions.retain(|r| {
            let keep = r.address >= end;
            end = end.max(r.address + r.bytes.len() as u64);
            keep
        });
        let tls = tls_callbacks(&b, &sections, h.tls);
        let mut starts: Vec<u64> = entry.into_iter().collect();
        starts.extend(&tls);
        let image = x86::Image {
            bits: 32,
            regions,
            starts,
            relocations: None,
            slots: thunks.iter().map(|&(slot, o)| (slot, never_returns(o))).collect(),
            base: Some(h.base),
        };
        let found = x86::follow(&image);

        let summary = Summary {
            format: Format::Xbe,
            format_name: "XBE".into(),
            kind: match keys {
                Some((build, ..)) => format!("Executable ({})", build.name()),
                None => "Executable".into(),
            },
            arch: "x86".into(),
            bits: 32,
            little_endian: true,
            file_size: len,
            entry,
            image_base: Some(h.base),
            build_id: None,
            debug_link: None,
            has_dwarf: false,
            has_symbols: !symbols.is_empty(),
            synthetic_addresses: false,
            section_count: sections.len() as u32,
            segment_count: segments.len() as u32,
            symbol_count: symbols.len() as u32,
            properties: properties(&b, &h, &imports, tls.len(), &found),
            fingerprint: crate::binary::fingerprint(bytes, None),
        };
        let mut binary = Binary {
            data: data.clone(),
            summary,
            sections,
            segments,
            symbols,
            imports,
            exports: Vec::new(),
            layout: Layout::empty(),
            machine: Machine::Other,
            arch: object::Architecture::I386,
            is64: false,
            endian: Endian::Little,
            image_base: h.base,
            debug: None,
            types_file: None,
            discovered: found.functions,
            code_tables: found.tables,
            code_switches: found.switches,
            code_parts: found.parts,
            debug_symbols: Default::default(),
            annotations: Vec::new(),
            strings: std::sync::OnceLock::new(),
            coverage: std::sync::OnceLock::new(),
            xrefs: std::sync::OnceLock::new(),
            pointers: std::sync::OnceLock::new(),
            objc: std::sync::OnceLock::new(),
            similar: std::sync::OnceLock::new(),
            rom: None,
            wasm: None,
        };
        binary.rebuild_static_symbols();
        binary.layout = binary.build_layout(Format::Xbe);
        Ok(binary)
    }
}

/// The facts to show first: the title, the consoles it is for, the XDK
/// that built it, what the certificate allows, and how the code was found.
fn properties(b: &Bytes, h: &Header, imports: &[Import], tls: usize, found: &x86::Found) -> Vec<Property> {
    let mut p = Vec::new();
    let mut add = |key: &str, value: String| {
        if !value.is_empty() {
            p.push(Property { key: key.into(), value });
        }
    };
    let cert = h.offset(h.certificate);
    let cert_word = |at: u64| cert.and_then(|c| b.u32(c + at));
    if let Some(title) = cert.and_then(|c| b.slice(c + 0x0C, 80)).map(utf16) {
        add("Title", title);
    }
    if let Some(id) = cert_word(0x08) {
        add("Title ID", title_id(id));
    }
    add(
        "Build",
        match h.keys() {
            Some((build, ..)) => format!(
                "{} (its keys decode the entry point and the kernel thunk table's address into the image)",
                build.name()
            ),
            None => "unknown (no known keys decode both the entry point and the kernel thunk table's address into \
                     the image)"
                .into(),
        },
    );
    // XAPILIB, the XDK's runtime, carries the XDK's build number.
    let libraries = libraries(b, h);
    let xapi = h
        .offset(h.xapi_library)
        .and_then(|o| Library::read(b, o))
        .filter(|l| !l.name.is_empty())
        .or_else(|| libraries.iter().find(|l| l.name == "XAPILIB").cloned());
    if let Some(x) = &xapi {
        let others: Vec<String> = libraries
            .iter()
            .filter(|l| l.build != x.build)
            .map(Library::describe)
            .collect();
        add(
            "Built with",
            format!(
                "XDK {} (from {}){}",
                x.build,
                x.describe(),
                if others.is_empty() {
                    String::new()
                } else {
                    format!("; libraries from other XDK builds: {}", others.join(", "))
                }
            ),
        );
    }
    add(
        "Libraries",
        libraries.iter().map(Library::describe).collect::<Vec<_>>().join(", "),
    );
    add("Timestamp", util::unix_time(h.timestamp));
    if !imports.is_empty() {
        let named = imports.iter().filter(|i| !i.name.starts_with(KERNEL)).count();
        add(
            "Kernel imports",
            format!(
                "{} from {KERNEL}, by ordinal ({})",
                imports.len(),
                if named == imports.len() {
                    "all named".to_string()
                } else {
                    format!("{named} named, {} not", imports.len() - named)
                }
            ),
        );
    }
    if let Some(r) = cert_word(0xA0) {
        add("Regions", regions(r as u64));
    }
    if let Some(m) = cert_word(0x9C) {
        add("Media", media(m as u64));
    }
    if let Some(v) = cert_word(0xAC) {
        add("Version", format!("{v:#x}"));
    }
    if let Some(d) = cert_word(0xA8).filter(|&d| d != 0) {
        add("Disk", d.to_string());
    }
    add("Initialization flags", init_flags(h.init_flags as u64));
    if let Some(path) = h.offset(h.debug_path).and_then(|o| b.cstr(o, 1024)) {
        add("Debug path", util::lossy(path));
    }
    let switches = found.tables.iter().filter(|t| t.entry == 4).count();
    add(
        "Code found",
        format!(
            "{} functions and {switches} jump table{}, following the code from the entry point{}, then from the \
             code addresses the image holds (an XBE has no base relocations: words that land in the code may be \
             addresses)",
            found.functions.len(),
            if switches == 1 { "" } else { "s" },
            if tls > 0 { " and TLS callbacks" } else { "" }
        ),
    );
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_exports_are_numbered_from_one() {
        // Ordinals cited on their own, where the table puts them.
        for (ordinal, name) in [
            (1, "AvGetSavedDataAddress"),
            (8, "DbgPrint"),
            (49, "HalReturnToFirmware"),
            (95, "KeBugCheck"),
            (96, "KeBugCheckEx"),
            (156, "KeTickCount"),
            (165, "MmAllocateContiguousMemory"),
            (187, "NtClose"),
            (190, "NtCreateFile"),
            (258, "PsTerminateSystemThread"),
            (289, "RtlInitAnsiString"),
            (327, "XeLoadSection"),
            (366, "HalWriteSMCScratchRegister"),
        ] {
            assert_eq!(kernel_export(ordinal), Some(name), "ordinal {ordinal}");
        }
        assert_eq!(kernel_export(0), None);
        assert_eq!(kernel_import_name(374), "xboxkrnl.exe #374");
        let mut names = KERNEL_EXPORTS.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), KERNEL_EXPORTS.len(), "a name twice");
        // The functions that never return are the ones meant.
        let never: Vec<&str> = (1..=366)
            .filter(|&o| never_returns(o))
            .filter_map(kernel_export)
            .collect();
        assert_eq!(
            never,
            [
                "HalReturnToFirmware",
                "KeBugCheck",
                "KeBugCheckEx",
                "PsTerminateSystemThread"
            ]
        );
    }

    #[test]
    fn keys_tell_retail_from_debug() {
        let mut data = vec![0u8; 0x200];
        data[..4].copy_from_slice(b"XBEH");
        let put = |data: &mut Vec<u8>, at: usize, v: u32| data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut data, 0x104, 0x10000);
        put(&mut data, 0x108, 0x200);
        put(&mut data, 0x10C, 0x40000);
        put(&mut data, 0x110, 0x178);
        for build in Build::ALL {
            put(&mut data, 0x128, 0x11000 ^ build.entry_key());
            put(&mut data, 0x158, 0x2F000 ^ build.thunk_key());
            let h = Header::read(&data).unwrap();
            assert_eq!(h.keys().map(|k| (k.0, k.1, k.2)), Some((build, 0x11000, 0x2F000)));
        }
        // The entry point decodes with one key, the thunk table with another: neither.
        put(&mut data, 0x128, 0x11000 ^ Build::Retail.entry_key());
        put(&mut data, 0x158, 0x2F000 ^ Build::Debug.thunk_key());
        assert!(Header::read(&data).unwrap().keys().is_none());
        assert!(is_xbe(&data));
        assert!(!is_xbe(&data[..0x100]));
        data[0] = b'M';
        assert!(!is_xbe(&data));
    }

    /// An XBE with what later XDKs add: a 0x184-byte image header (library
    /// features), a 0x1EC-byte certificate; and a non-kernel import
    /// directory, as a development build has. One section: `ret`, then the
    /// kernel thunk table (DbgPrint).
    fn later_xbe() -> Vec<u8> {
        let mut d = vec![0u8; 0x1000];
        let put = |d: &mut Vec<u8>, at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let base = 0x10000;
        let (cert, headers, names, counts, libraries, features, imports, name, end) =
            (0x184, 0x370, 0x3A8, 0x3B0, 0x3B4, 0x3C4, 0x3E4, 0x3F4, 0x408);
        d[..4].copy_from_slice(b"XBEH");
        for (at, v) in [
            (0x104, base),
            (0x108, end),
            (0x10C, 0x1020),
            (0x110, 0x184),
            (0x118, base + cert),
            (0x11C, 1),
            (0x120, base + headers),
            (0x128, 0x11000 ^ Build::Debug.entry_key()),
            (0x158, 0x11010 ^ Build::Debug.thunk_key()),
            (0x15C, base + imports),
            (0x160, 1),
            (0x164, base + libraries),
            (0x16C, base + libraries),
            (0x178, base + features),
            (0x17C, 2),
            (cert as usize, 0x1EC),
            (headers as usize, PRELOAD | EXECUTABLE),
            (headers as usize + 0x04, 0x11000),
            (headers as usize + 0x08, 0x20),
            (headers as usize + 0x0C, 0x1000),
            (headers as usize + 0x10, 0x20),
            (headers as usize + 0x14, base + names),
            (headers as usize + 0x1C, base + counts),
            (headers as usize + 0x20, base + counts + 2),
            (imports as usize, 0x11018),
            (imports as usize + 4, base + name),
        ] {
            put(&mut d, at, v);
        }
        d[names as usize..][..6].copy_from_slice(b".text\0");
        for at in [libraries, features, features + 16] {
            let at = at as usize;
            d[at..at + 8].copy_from_slice(b"XAPILIB\0");
            d[at + 8..at + 16].copy_from_slice(&[1, 0, 0, 0, 0xD9, 0x16, 0, 0x40]);
        }
        for (i, c) in "xbdm.dll".encode_utf16().enumerate() {
            d[name as usize + 2 * i..][..2].copy_from_slice(&c.to_le_bytes());
        }
        // The section: ret, then the thunk table at 0x11010, then the imports' at 0x11018.
        d.extend([0xC3]);
        d.resize(0x1010, 0xCC);
        d.extend(0x8000_0008u32.to_le_bytes());
        d.resize(0x1020, 0);
        d
    }

    #[test]
    fn what_later_xdks_add_is_laid_out() {
        let bin = Binary::parse(later_xbe()).unwrap();
        let s = bin.summary();
        assert_eq!((s.kind.as_str(), s.entry), ("Executable (debug)", Some(0x11000)));
        assert_eq!(bin.imports()[0].name, "DbgPrint");
        let at = |offset: u64| -> Vec<String> { bin.describe_offset(offset).into_iter().map(|p| p.name).collect() };
        assert_eq!(at(0x178), ["Headers", "Image header", "Library features address"]);
        assert_eq!(at(0x180), ["Headers", "Image header", "Debug info address"]);
        assert_eq!(at(0x184 + 0x1D4), ["Headers", "Certificate", "Online service ID"]);
        assert_eq!(at(0x3D4)[..3], ["Headers", "Library features", "XAPILIB 1.0.5849"]);
        assert_eq!(at(0x3E8)[..2], ["Headers", "Non-kernel import directory"]);
        assert_eq!(at(0x3F6), ["Headers", "Import name"]);
        let unknown: u64 = bin
            .composition()
            .iter()
            .filter(|(k, _)| *k == RegionKind::Unknown)
            .map(|(_, n)| n)
            .sum();
        assert_eq!(unknown, 0);
        // Every byte once: the spans run from the start of the file to its end.
        let mut cursor = 0;
        for span in bin.spans(0, bin.data().len() as u64) {
            assert_eq!(span.start, cursor, "a gap or an overlap at {cursor:#x}");
            cursor = span.end;
        }
        assert_eq!(cursor, bin.data().len() as u64);
    }

    #[test]
    fn titles_and_flags_read_as_written() {
        assert_eq!(title_id(0x4D53_0004), "4D530004 (MS-004)");
        assert_eq!(title_id(0x0000_0001), "00000001");
        assert_eq!(regions(0x8000_0003), "North America, Japan, manufacturing");
        assert_eq!(section_flags(0x46), "preload, executable, 0x40");
        assert_eq!(library_flags(0xC001), "QFE 1, approved, debug build (0xc001)");
        assert_eq!(utf16(&[b'H', 0, b'i', 0, 0, 0, b'x', 0]), "Hi");
    }
}
