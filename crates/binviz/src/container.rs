//! Files that contain several binaries: universal (fat) Mach-O and ar archives.

use std::sync::Arc;

use object::read::archive::ArchiveFile;
use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};
use serde::Serialize;

use crate::binary::Binary;
use crate::error::{Error, Result, bail};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub index: u32,
    pub name: String,
    pub offset: u64,
    pub size: u64,
    /// Architecture of a fat slice, or detected for an archive member.
    pub arch: Option<String>,
    #[serde(skip)]
    pub(crate) cputype: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInfo {
    /// "Universal Mach-O" or "Archive".
    pub kind: String,
    pub file_size: u64,
    pub members: Vec<Member>,
}

pub struct Container {
    data: Arc<[u8]>,
    info: ContainerInfo,
}

fn cpu_name(cputype: u32) -> String {
    match object::macho::CpuType(cputype) {
        object::macho::CPU_TYPE_X86_64 => "x86-64".into(),
        object::macho::CPU_TYPE_X86 => "x86".into(),
        object::macho::CPU_TYPE_ARM64 => "arm64".into(),
        object::macho::CPU_TYPE_ARM64_32 => "arm64_32".into(),
        object::macho::CPU_TYPE_ARM => "arm".into(),
        object::macho::CPU_TYPE_POWERPC => "ppc".into(),
        object::macho::CPU_TYPE_POWERPC64 => "ppc64".into(),
        other => other.name().map_or_else(|| format!("{cputype:#x}"), str::to_string),
    }
}

impl Container {
    /// True if the data is a fat Mach-O or an archive.
    pub fn is_container(data: &[u8]) -> bool {
        matches!(
            object::FileKind::parse(data),
            Ok(object::FileKind::MachOFat32 | object::FileKind::MachOFat64 | object::FileKind::Archive)
        )
    }

    pub fn parse(data: impl Into<Arc<[u8]>>) -> Result<Container> {
        let data: Arc<[u8]> = data.into();
        let bytes: &[u8] = &data;
        let mut members = Vec::new();
        let kind = match object::FileKind::parse(bytes) {
            Ok(object::FileKind::MachOFat32) => {
                let fat = MachOFatFile32::parse(bytes)?;
                for (i, arch) in fat.arches().iter().enumerate() {
                    push_fat(&mut members, i, arch);
                }
                "Universal Mach-O"
            }
            Ok(object::FileKind::MachOFat64) => {
                let fat = MachOFatFile64::parse(bytes)?;
                for (i, arch) in fat.arches().iter().enumerate() {
                    push_fat(&mut members, i, arch);
                }
                "Universal Mach-O"
            }
            Ok(object::FileKind::Archive) => {
                let archive = ArchiveFile::parse(bytes)?;
                for m in archive.members() {
                    let m = m?;
                    let name = String::from_utf8_lossy(m.name()).into_owned();
                    let (offset, size) = m.file_range();
                    let arch = m.data(bytes).ok().and_then(|d| object::File::parse(d).ok()).map(|f| {
                        use object::Object;
                        format!("{:?}", f.architecture())
                    });
                    members.push(Member {
                        index: members.len() as u32,
                        name,
                        offset,
                        size,
                        arch,
                        cputype: None,
                    });
                }
                "Archive"
            }
            _ => bail!("not a universal binary or archive"),
        };
        Ok(Container {
            info: ContainerInfo {
                kind: kind.into(),
                file_size: bytes.len() as u64,
                members,
            },
            data,
        })
    }

    pub fn info(&self) -> &ContainerInfo {
        &self.info
    }

    pub fn members(&self) -> &[Member] {
        &self.info.members
    }

    pub fn member_data(&self, index: u32) -> Result<Arc<[u8]>> {
        let m = self
            .info
            .members
            .get(index as usize)
            .ok_or_else(|| Error::new("no such member"))?;
        let start = m.offset as usize;
        let end = start
            .checked_add(m.size as usize)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| Error::new("member extends past end of file"))?;
        Ok(Arc::from(&self.data[start..end]))
    }

    /// Parses one member as a binary. Offsets in the result are relative to the member.
    pub fn open(&self, index: u32) -> Result<Binary> {
        Binary::parse(self.member_data(index)?)
    }
}

fn push_fat<A: FatArch>(members: &mut Vec<Member>, i: usize, arch: &A) {
    let cputype = arch.cputype().0;
    members.push(Member {
        index: i as u32,
        name: cpu_name(cputype),
        offset: arch.offset().into(),
        size: arch.size().into(),
        arch: Some(cpu_name(cputype)),
        cputype: Some(cputype),
    });
}
