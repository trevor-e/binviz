//! The Rich header: what Microsoft's linker records, between the DOS stub and
//! the PE header, about the tools that made the objects it linked. Each entry
//! names a product — the C compiler, the C++ compiler, the assembler, the
//! linker, the resource converter… of one Visual C++ release — with the
//! tool's build number and how many of the image's objects it made. For a
//! matching decompilation, that says which compiler (down to the service
//! pack) the code has to be rebuilt with.
//!
//! The entries are XOR-masked with a key that is also a checksum of the DOS
//! header, the stub and the entries themselves, so a header edited after
//! linking shows.

/// Product IDs, as Microsoft's tools name them (the `prodid` enumeration):
/// the index is the ID. `Utc` is the compiler back end (`cl.exe`), numbered
/// by its own version (`Utc12` is cl 12.00, Visual C++ 6.0); the other tools
/// are numbered by the Visual C++ version (`Linker600`).
const PRODUCTS: [&str; 0x10F] = [
    "Unknown",
    "Import0",
    "Linker510",
    "Cvtomf510",
    "Linker600",
    "Cvtomf600",
    "Cvtres500",
    "Utc11_Basic",
    "Utc11_C",
    "Utc12_Basic",
    "Utc12_C",
    "Utc12_CPP",
    "AliasObj60",
    "VisualBasic60",
    "Masm613",
    "Masm710",
    "Linker511",
    "Cvtomf511",
    "Masm614",
    "Linker512",
    "Cvtomf512",
    "Utc12_C_Std",
    "Utc12_CPP_Std",
    "Utc12_C_Book",
    "Utc12_CPP_Book",
    "Implib700",
    "Cvtomf700",
    "Utc13_Basic",
    "Utc13_C",
    "Utc13_CPP",
    "Linker610",
    "Cvtomf610",
    "Linker601",
    "Cvtomf601",
    "Utc12_1_Basic",
    "Utc12_1_C",
    "Utc12_1_CPP",
    "Linker620",
    "Cvtomf620",
    "AliasObj70",
    "Linker621",
    "Cvtomf621",
    "Masm615",
    "Utc13_LTCG_C",
    "Utc13_LTCG_CPP",
    "Masm620",
    "ILAsm100",
    "Utc12_2_Basic",
    "Utc12_2_C",
    "Utc12_2_CPP",
    "Utc12_2_C_Std",
    "Utc12_2_CPP_Std",
    "Utc12_2_C_Book",
    "Utc12_2_CPP_Book",
    "Implib622",
    "Cvtomf622",
    "Cvtres501",
    "Utc13_C_Std",
    "Utc13_CPP_Std",
    "Cvtpgd1300",
    "Linker622",
    "Linker700",
    "Export622",
    "Export700",
    "Masm700",
    "Utc13_POGO_I_C",
    "Utc13_POGO_I_CPP",
    "Utc13_POGO_O_C",
    "Utc13_POGO_O_CPP",
    "Cvtres700",
    "Cvtres710p",
    "Linker710p",
    "Cvtomf710p",
    "Export710p",
    "Implib710p",
    "Masm710p",
    "Utc1310p_C",
    "Utc1310p_CPP",
    "Utc1310p_C_Std",
    "Utc1310p_CPP_Std",
    "Utc1310p_LTCG_C",
    "Utc1310p_LTCG_CPP",
    "Utc1310p_POGO_I_C",
    "Utc1310p_POGO_I_CPP",
    "Utc1310p_POGO_O_C",
    "Utc1310p_POGO_O_CPP",
    "Linker624",
    "Cvtomf624",
    "Export624",
    "Implib624",
    "Linker710",
    "Cvtomf710",
    "Export710",
    "Implib710",
    "Cvtres710",
    "Utc1310_C",
    "Utc1310_CPP",
    "Utc1310_C_Std",
    "Utc1310_CPP_Std",
    "Utc1310_LTCG_C",
    "Utc1310_LTCG_CPP",
    "Utc1310_POGO_I_C",
    "Utc1310_POGO_I_CPP",
    "Utc1310_POGO_O_C",
    "Utc1310_POGO_O_CPP",
    "AliasObj710",
    "AliasObj710p",
    "Cvtpgd1310",
    "Cvtpgd1310p",
    "Utc1400_C",
    "Utc1400_CPP",
    "Utc1400_C_Std",
    "Utc1400_CPP_Std",
    "Utc1400_LTCG_C",
    "Utc1400_LTCG_CPP",
    "Utc1400_POGO_I_C",
    "Utc1400_POGO_I_CPP",
    "Utc1400_POGO_O_C",
    "Utc1400_POGO_O_CPP",
    "Cvtpgd1400",
    "Linker800",
    "Cvtomf800",
    "Export800",
    "Implib800",
    "Cvtres800",
    "Masm800",
    "AliasObj800",
    "PhoenixPrerelease",
    "Utc1400_CVTCIL_C",
    "Utc1400_CVTCIL_CPP",
    "Utc1400_LTCG_MSIL",
    "Utc1500_C",
    "Utc1500_CPP",
    "Utc1500_C_Std",
    "Utc1500_CPP_Std",
    "Utc1500_CVTCIL_C",
    "Utc1500_CVTCIL_CPP",
    "Utc1500_LTCG_C",
    "Utc1500_LTCG_CPP",
    "Utc1500_LTCG_MSIL",
    "Utc1500_POGO_I_C",
    "Utc1500_POGO_I_CPP",
    "Utc1500_POGO_O_C",
    "Utc1500_POGO_O_CPP",
    "Cvtpgd1500",
    "Linker900",
    "Export900",
    "Implib900",
    "Cvtres900",
    "Masm900",
    "AliasObj900",
    "Resource",
    "AliasObj1000",
    "Cvtpgd1600",
    "Cvtres1000",
    "Export1000",
    "Implib1000",
    "Linker1000",
    "Masm1000",
    "Phx1600_C",
    "Phx1600_CPP",
    "Phx1600_CVTCIL_C",
    "Phx1600_CVTCIL_CPP",
    "Phx1600_LTCG_C",
    "Phx1600_LTCG_CPP",
    "Phx1600_LTCG_MSIL",
    "Phx1600_POGO_I_C",
    "Phx1600_POGO_I_CPP",
    "Phx1600_POGO_O_C",
    "Phx1600_POGO_O_CPP",
    "Utc1600_C",
    "Utc1600_CPP",
    "Utc1600_CVTCIL_C",
    "Utc1600_CVTCIL_CPP",
    "Utc1600_LTCG_C",
    "Utc1600_LTCG_CPP",
    "Utc1600_LTCG_MSIL",
    "Utc1600_POGO_I_C",
    "Utc1600_POGO_I_CPP",
    "Utc1600_POGO_O_C",
    "Utc1600_POGO_O_CPP",
    "AliasObj1010",
    "Cvtpgd1610",
    "Cvtres1010",
    "Export1010",
    "Implib1010",
    "Linker1010",
    "Masm1010",
    "Utc1610_C",
    "Utc1610_CPP",
    "Utc1610_CVTCIL_C",
    "Utc1610_CVTCIL_CPP",
    "Utc1610_LTCG_C",
    "Utc1610_LTCG_CPP",
    "Utc1610_LTCG_MSIL",
    "Utc1610_POGO_I_C",
    "Utc1610_POGO_I_CPP",
    "Utc1610_POGO_O_C",
    "Utc1610_POGO_O_CPP",
    "AliasObj1100",
    "Cvtpgd1700",
    "Cvtres1100",
    "Export1100",
    "Implib1100",
    "Linker1100",
    "Masm1100",
    "Utc1700_C",
    "Utc1700_CPP",
    "Utc1700_CVTCIL_C",
    "Utc1700_CVTCIL_CPP",
    "Utc1700_LTCG_C",
    "Utc1700_LTCG_CPP",
    "Utc1700_LTCG_MSIL",
    "Utc1700_POGO_I_C",
    "Utc1700_POGO_I_CPP",
    "Utc1700_POGO_O_C",
    "Utc1700_POGO_O_CPP",
    "AliasObj1200",
    "Cvtpgd1800",
    "Cvtres1200",
    "Export1200",
    "Implib1200",
    "Linker1200",
    "Masm1200",
    "Utc1800_C",
    "Utc1800_CPP",
    "Utc1800_CVTCIL_C",
    "Utc1800_CVTCIL_CPP",
    "Utc1800_LTCG_C",
    "Utc1800_LTCG_CPP",
    "Utc1800_LTCG_MSIL",
    "Utc1800_POGO_I_C",
    "Utc1800_POGO_I_CPP",
    "Utc1800_POGO_O_C",
    "Utc1800_POGO_O_CPP",
    "AliasObj1210",
    "Cvtpgd1810",
    "Cvtres1210",
    "Export1210",
    "Implib1210",
    "Linker1210",
    "Masm1210",
    "Utc1810_C",
    "Utc1810_CPP",
    "Utc1810_CVTCIL_C",
    "Utc1810_CVTCIL_CPP",
    "Utc1810_LTCG_C",
    "Utc1810_LTCG_CPP",
    "Utc1810_LTCG_MSIL",
    "Utc1810_POGO_I_C",
    "Utc1810_POGO_I_CPP",
    "Utc1810_POGO_O_C",
    "Utc1810_POGO_O_CPP",
    "AliasObj1400",
    "Cvtpgd1900",
    "Cvtres1400",
    "Export1400",
    "Implib1400",
    "Linker1400",
    "Masm1400",
    "Utc1900_C",
    "Utc1900_CPP",
    "Utc1900_CVTCIL_C",
    "Utc1900_CVTCIL_CPP",
    "Utc1900_LTCG_C",
    "Utc1900_LTCG_CPP",
    "Utc1900_LTCG_MSIL",
    "Utc1900_POGO_I_C",
    "Utc1900_POGO_I_CPP",
    "Utc1900_POGO_O_C",
    "Utc1900_POGO_O_CPP",
];

/// One entry of the Rich header, decoded.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tool {
    pub product: u16,
    pub build: u16,
    pub count: u32,
    /// The product's name in Microsoft's list (`Utc12_C`), if the ID is known.
    pub name: Option<&'static str>,
    /// What the tool is: "C compiler", "linker", "assembler"…
    pub what: String,
    /// The tool's own version with the build (`12.00.8804`), when the product names one.
    pub version: Option<String>,
    /// The Visual C++ release the tool shipped with (`Visual C++ 6.0 SP5`), when that is known.
    pub release: Option<String>,
    /// A compiler: its objects are the code a decompilation rebuilds.
    pub compiler: bool,
}

impl Tool {
    /// The entry in a few words: `cl 12.00.8804, C compiler (Visual C++ 6.0 SP5): 41 objects`.
    pub fn describe(&self) -> String {
        let mut out = match (&self.version, self.compiler) {
            (Some(v), true) => format!("cl {v}, {}", self.what),
            (Some(v), false) => format!("{} {v}", self.what),
            (None, _) => self.what.clone(),
        };
        if let Some(r) = &self.release {
            out.push_str(&format!(" ({r})"));
        }
        let unit = match self.product {
            1 => "imported functions",
            _ if self.count == 1 => "object",
            _ => "objects",
        };
        out.push_str(&format!(": {} {unit}", self.count));
        if let Some(n) = self.name {
            out.push_str(&format!(" [{n}]"));
        } else {
            out.push_str(&format!(" [product {:#06x}, build {}]", self.product, self.build));
        }
        out
    }
}

/// Decodes one entry: the product ID (`comp_id >> 16`), the build and the count.
pub(crate) fn tool(product: u16, build: u16, count: u32) -> Tool {
    let name = PRODUCTS.get(product as usize).copied();
    let mut t = Tool {
        product,
        build,
        count,
        name,
        what: String::new(),
        version: None,
        release: None,
        compiler: false,
    };
    let Some(name) = name else {
        t.what = "unknown tool".into();
        return t;
    };
    match name {
        "Unknown" => {
            t.what = "objects no tool marked".into();
            return t;
        }
        "Import0" => {
            t.what = "imports".into();
            return t;
        }
        "Resource" => {
            t.what = "resources".into();
            return t;
        }
        "PhoenixPrerelease" => {
            t.what = "Phoenix compiler (prerelease)".into();
            return t;
        }
        _ => {}
    }
    // `Utc1310p_LTCG_CPP`: family, number, then what the compiler made.
    let split = name.find(|c: char| c.is_ascii_digit()).unwrap_or(name.len());
    let (family, rest) = name.split_at(split);
    let (number, kind) = match rest.find('_') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    let prerelease = number.ends_with('p');
    let digits = number.trim_end_matches('p');
    // `Utc12_1_C` and `Utc12_2_C` are later builds of cl 12.00.
    let kind = kind.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
    let compiler_family = matches!(family, "Utc" | "Phx");
    // Versions are written without their dot: `600` is 6.00, `1310` is 13.10;
    // two digits are a compiler's major version (`Utc12`: 12.00), else a
    // version and its tenth (`AliasObj60`: 6.0).
    let version = |d: &str| -> Option<(u32, u32)> {
        let n: u32 = d.parse().ok()?;
        Some(match d.len() {
            1 => (n, 0),
            2 if compiler_family => (n, 0),
            2 => (n / 10, n % 10 * 10),
            _ => (n / 100, n % 100),
        })
    };
    let Some((major, minor)) = version(digits) else {
        t.what = name.to_string();
        return t;
    };
    // The Visual C++ release: the compiler's version is 6 ahead of it (cl 12 is Visual C++ 6).
    let vc = if compiler_family || family == "Cvtpgd" {
        (major.saturating_sub(6), minor)
    } else {
        (major, minor)
    };
    t.what = match family {
        "Utc" | "Phx" => {
            let lang = if kind.contains("CPP") {
                "C++ compiler"
            } else if kind.contains("MSIL") {
                "compiler, managed code"
            } else if kind.contains("Basic") {
                "Visual Basic compiler back end"
            } else {
                "C compiler"
            };
            let how = if kind.contains("LTCG") {
                ", link-time code generation"
            } else if kind.contains("POGO_I") {
                ", profile-guided instrumented"
            } else if kind.contains("POGO_O") {
                ", profile-guided optimized"
            } else if kind.contains("CVTCIL") {
                ", from CIL"
            } else if kind.contains("Std") {
                ", Standard Edition"
            } else if kind.contains("Book") {
                ", Learning Edition"
            } else {
                ""
            };
            t.compiler = true;
            format!("{lang}{how}{}", if family == "Phx" { " (Phoenix)" } else { "" })
        }
        "Linker" => "link".into(),
        "Masm" => "ml (assembler)".into(),
        "Cvtres" => "cvtres (resources)".into(),
        "Cvtomf" => "cvtomf (OMF converter)".into(),
        "Export" => "lib (exports)".into(),
        "Implib" => "lib (import library)".into(),
        "AliasObj" => "aliasobj".into(),
        "Cvtpgd" => "pgocvt (profile data)".into(),
        "ILAsm" => "ilasm".into(),
        "VisualBasic" => "Visual Basic".into(),
        _ => name.to_string(),
    };
    if prerelease {
        t.what.push_str(", prerelease");
    }
    t.version = Some(format!("{major}.{minor:02}.{build}"));
    // The assembler and the resource tools of the 5.x and 6.x days shipped
    // with more than one release; from 7.0 on, each release had its own.
    if compiler_family || matches!(family, "Linker" | "Cvtpgd") || vc.0 >= 7 {
        t.release = release(vc, build);
    }
    t
}

/// The Visual C++ release with version `vc` (the linker's numbering: 6.00
/// for Visual C++ 6.0), sharpened by the tool's build where it names a
/// service pack or update.
fn release(vc: (u32, u32), build: u16) -> Option<String> {
    // There was no Visual C++ 13: cl 19 (19 - 6) is Visual C++ 14.
    let vc = if vc == (13, 0) { (14, 0) } else { vc };
    let name = match vc {
        (5, _) => "Visual C++ 5.0",
        (6, _) => "Visual C++ 6.0",
        (7, 0) => "Visual C++ .NET 2002",
        (7, 10) => "Visual C++ .NET 2003",
        (8, 0) => "Visual C++ 2005",
        (9, 0) => "Visual C++ 2008",
        (10, 0) => "Visual C++ 2010",
        (11, 0) => "Visual C++ 2012",
        (12, 0) => "Visual C++ 2013",
        // Visual C++ 2015 and every release after it kept version 14 (cl 19); the build tells them apart.
        (14, 0) => match build {
            0..25000 => "Visual C++ 2015",
            25000..27500 => "Visual C++ 2017",
            27500..30500 => "Visual C++ 2019",
            _ => "Visual C++ 2022 or later",
        },
        _ => return None,
    };
    // Builds that name a service pack or an update.
    let detail = match (vc.0, build) {
        (6, 8168) => " (no service pack)",
        (6, 8804) => " SP5 or SP6",
        (7, 3077) if vc.1 == 10 => " (no service pack)",
        (7, 6030) if vc.1 == 10 => " SP1",
        (9, 21022) => " (no service pack)",
        (9, 30729) => " SP1",
        (10, 30319) => " (no service pack)",
        (10, 40219) => " SP1",
        (11, 50727) => " (no update)",
        (11, 51106) => " Update 1",
        (11, 60315) => " Update 2",
        (11, 60610) => " Update 3",
        (11, 61030) => " Update 4",
        (12, 21005) => " (no update)",
        (12, 30501) => " Update 2",
        (12, 30723) => " Update 3",
        (12, 31101) => " Update 4",
        (12, 40629) => " Update 5",
        (14, 23026) => " (no update)",
        (14, 23506) => " Update 1",
        (14, 23918) => " Update 2",
        (14, 24210 | 24215) => " Update 3",
        _ => "",
    };
    Some(format!("{name}{detail}"))
}

/// The Rich header's checksum: the offset of its start, then every byte
/// before it (the DOS header without `e_lfanew`, the stub) rotated left by
/// its offset, then each entry's `comp_id` rotated left by its count. An
/// untouched header's key is this.
pub(crate) fn checksum(data: &[u8], start: usize, entries: &[(u32, u32)]) -> u32 {
    let mut sum = start as u32;
    for (i, &b) in data.iter().enumerate().take(start) {
        if (0x3C..0x40).contains(&i) {
            continue;
        }
        sum = sum.wrapping_add((b as u32).rotate_left(i as u32 % 32));
    }
    for &(comp_id, count) in entries {
        sum = sum.wrapping_add(comp_id.rotate_left(count % 32));
    }
    sum
}

/// What the Rich header says about the toolchain, in a line: the compiler
/// that made most of the objects, and the linker.
pub(crate) fn summary(tools: &[Tool]) -> Option<String> {
    let compilers: Vec<&Tool> = tools.iter().filter(|t| t.compiler && t.count > 0).collect();
    let linker = tools.iter().find(|t| t.name.is_some_and(|n| n.starts_with("Linker")));
    let main = compilers.iter().max_by_key(|t| (t.count, t.product))?;
    let version = main.version.as_deref()?;
    // All the objects that compiler build made, C and C++.
    let same: Vec<&&Tool> = compilers
        .iter()
        .filter(|t| t.version.as_deref() == Some(version))
        .collect();
    let c: u32 = same
        .iter()
        .filter(|t| t.what.starts_with("C compiler"))
        .map(|t| t.count)
        .sum();
    let cpp: u32 = same.iter().filter(|t| t.what.starts_with("C++")).map(|t| t.count).sum();
    let mut made = Vec::new();
    if c > 0 {
        made.push(format!("{c} C"));
    }
    if cpp > 0 {
        made.push(format!("{cpp} C++"));
    }
    let mut out = format!(
        "{}cl {version} ({} objects)",
        main.release.as_deref().map_or(String::new(), |r| format!("{r}: ")),
        made.join(" and ")
    );
    let others: Vec<String> = compilers
        .iter()
        .filter(|t| t.version.as_deref() != Some(version))
        .filter_map(|t| t.version.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !others.is_empty() {
        out.push_str(&format!("; also cl {}", others.join(", ")));
    }
    if let Some(l) = linker.and_then(|l| l.version.as_deref()) {
        out.push_str(&format!("; link {l}"));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_read_as_tools_and_releases() {
        let cl = tool(0x000A, 8804, 41);
        assert_eq!(
            (
                cl.name,
                cl.what.as_str(),
                cl.version.as_deref(),
                cl.release.as_deref(),
                cl.compiler
            ),
            (
                Some("Utc12_C"),
                "C compiler",
                Some("12.00.8804"),
                Some("Visual C++ 6.0 SP5 or SP6"),
                true
            )
        );
        assert_eq!(
            cl.describe(),
            "cl 12.00.8804, C compiler (Visual C++ 6.0 SP5 or SP6): 41 objects [Utc12_C]"
        );
        let later = tool(0x0033, 8804, 2);
        assert_eq!(
            (later.what.as_str(), later.version.as_deref()),
            ("C++ compiler, Standard Edition", Some("12.00.8804"))
        );
        let link = tool(0x0004, 8447, 1);
        assert_eq!(link.describe(), "link 6.00.8447 (Visual C++ 6.0): 1 object [Linker600]");
        assert_eq!(tool(0x000C, 1, 1).version.as_deref(), Some("6.00.1"));
        assert_eq!(tool(0x0012, 8444, 2).release, None);
        assert_eq!(tool(0x0040, 9466, 2).release.as_deref(), Some("Visual C++ .NET 2002"));
        let ltcg = tool(0x0109, 30133, 7);
        assert_eq!(ltcg.what, "C++ compiler, link-time code generation");
        assert_eq!(ltcg.release.as_deref(), Some("Visual C++ 2019"));
        assert_eq!(
            tool(0x0105, 24215, 1).release.as_deref(),
            Some("Visual C++ 2015 Update 3")
        );
        assert_eq!(tool(0x0102, 26715, 1).release.as_deref(), Some("Visual C++ 2017"));
        assert_eq!(
            tool(0x005F, 6030, 9).release.as_deref(),
            Some("Visual C++ .NET 2003 SP1")
        );
        assert_eq!(
            tool(0x0001, 0, 136).describe(),
            "imports: 136 imported functions [Import0]"
        );
        assert_eq!(tool(0x0200, 1, 1).what, "unknown tool");
        assert_eq!(tool(0x0071, 50727, 1).version.as_deref(), Some("14.00.50727"));
        assert_eq!(tool(0x0071, 50727, 1).release.as_deref(), Some("Visual C++ 2005"));
    }

    #[test]
    fn the_summary_names_the_main_compiler_and_the_linker() {
        let tools = [
            tool(0x0001, 0, 90),
            tool(0x000A, 8804, 41),
            tool(0x000B, 8804, 3),
            tool(0x0012, 8444, 2),
            tool(0x0004, 8447, 1),
        ];
        assert_eq!(
            summary(&tools).unwrap(),
            "Visual C++ 6.0 SP5 or SP6: cl 12.00.8804 (41 C and 3 C++ objects); link 6.00.8447"
        );
        assert_eq!(summary(&[tool(0x0004, 8447, 1)]), None);
    }
}
