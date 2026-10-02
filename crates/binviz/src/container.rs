//! Files that contain several binaries: universal (fat) Mach-O, ar
//! archives, and CD images (a PlayStation game's disc, its files and the
//! executable it boots first).

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
    /// Architecture of a fat slice, or detected for an archive member (on a disc: what the file is).
    pub arch: Option<String>,
    /// Its bytes are `offset..offset + size` of the file (not on a raw CD image, sector by sector).
    pub contiguous: bool,
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
    /// A disc's sectors and files, by member index.
    disc: Option<(crate::disc::Layout, Vec<crate::disc::DiscFile>)>,
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
    /// True if the data is a fat Mach-O, an archive, or a CD image.
    pub fn is_container(data: &[u8]) -> bool {
        matches!(
            object::FileKind::parse(data),
            Ok(object::FileKind::MachOFat32 | object::FileKind::MachOFat64 | object::FileKind::Archive)
        ) || crate::disc::layout(&data[..data.len().min(crate::disc::PREFIX)]).is_some()
    }

    pub fn parse(data: impl Into<Arc<[u8]>>) -> Result<Container> {
        let data: Arc<[u8]> = data.into();
        let bytes: &[u8] = &data;
        if let Some(layout) = crate::disc::layout(&bytes[..bytes.len().min(crate::disc::PREFIX)]) {
            let disc = crate::disc::list(layout, &mut crate::disc::sectors(bytes, layout))?;
            let files: Vec<crate::disc::DiscFile> = disc.files.into_iter().filter(|f| !f.dir).collect();
            let members = files
                .iter()
                .enumerate()
                .map(|(i, f)| Member {
                    index: i as u32,
                    name: f.path.clone(),
                    offset: layout.offset(f.lba),
                    size: f.size,
                    arch: disc
                        .boot
                        .as_deref()
                        .filter(|b| b.eq_ignore_ascii_case(&f.path))
                        .map(|_| "boots first".into()),
                    cputype: None,
                    contiguous: layout.sector == 2048,
                })
                .collect();
            return Ok(Container {
                info: ContainerInfo {
                    kind: if disc.volume.is_empty() {
                        "CD image".into()
                    } else {
                        format!("CD image ({})", disc.volume)
                    },
                    file_size: bytes.len() as u64,
                    members,
                },
                data,
                disc: Some((layout, files)),
            });
        }
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
                        contiguous: true,
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
            disc: None,
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
        if let Some((layout, files)) = &self.disc {
            return Ok(crate::disc::extract(&self.data, *layout, &files[index as usize])?.into());
        }
        let start = m.offset as usize;
        let end = start
            .checked_add(m.size as usize)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| Error::new("member extends past end of file"))?;
        Ok(Arc::from(&self.data[start..end]))
    }

    /// The member a user means: its number, or its name (a disc file's path, in any case,
    /// with or without the folders in front).
    pub fn find(&self, text: &str) -> Option<u32> {
        let text = text.trim().trim_start_matches(['/', '\\']);
        if let Ok(i) = text.parse::<u32>()
            && (i as usize) < self.info.members.len()
        {
            return Some(i);
        }
        let text = text.replace('\\', "/");
        let text = text.split(';').next().unwrap_or(&text).to_string();
        let same = |name: &str| {
            let name = name.split(';').next().unwrap_or(name);
            name.eq_ignore_ascii_case(&text)
        };
        let tail = |name: &str| {
            name.rsplit('/')
                .next()
                .is_some_and(|last| last.split(';').next().unwrap_or(last).eq_ignore_ascii_case(&text))
        };
        let full = self.info.members.iter().position(|m| same(&m.name));
        full.or_else(|| self.info.members.iter().position(|m| tail(&m.name)))
            .map(|i| i as u32)
    }

    /// Every member on a line: number, where it starts (a disc's sector, else its offset), size, name.
    pub fn listing(&self) -> String {
        let mut out = format!("{} with {} members:\n", self.info.kind, self.info.members.len());
        for m in &self.info.members {
            let at = match &self.disc {
                Some((_, files)) => format!("sector {:>7}", files[m.index as usize].lba),
                None => format!("@{:#010x}", m.offset),
            };
            let note = m.arch.as_deref().map_or(String::new(), |a| format!("  ({a})"));
            out.push_str(&format!(
                "  [{}] {at} {:>11} bytes  {}{note}\n",
                m.index, m.size, m.name
            ));
        }
        out
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
        contiguous: true,
    });
}
