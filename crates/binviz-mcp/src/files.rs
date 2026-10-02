//! The files inside a file: a disc image's, an archive's members, and the
//! code inside an archive binviz has no reader for (a game's overlays).

use std::path::Path;

use serde_json::Value;

use crate::tools::{Server, int, string};

/// The bytes of `path`, as one file.
fn read(path: &str) -> Result<Vec<u8>, String> {
    binviz::read_file(Path::new(path))
        .map(|d| d.to_vec())
        .map_err(|e| format!("{path}: {e}"))
}

impl Server {
    pub(crate) fn list_disc_files(&mut self, args: &Value) -> Result<String, String> {
        let path = string(args, "path").ok_or("path is required")?;
        let data = read(path)?;
        if !binviz::Container::is_container(&data) {
            return Err(format!(
                "{path} holds no files: it is not a disc image, universal binary or archive"
            ));
        }
        let c = binviz::Container::parse(data).map_err(|e| e.to_string())?;
        Ok(c.listing())
    }

    pub(crate) fn extract_disc_file(&mut self, args: &Value) -> Result<String, String> {
        let path = string(args, "path").ok_or("path is required")?;
        let out = string(args, "out").ok_or("out is required: where to write the bytes")?;
        let data = read(path)?;
        let bytes: Vec<u8> = match (string(args, "member"), args.get("offset").and_then(Value::as_u64)) {
            (_, Some(from)) => {
                let len = args
                    .get("length")
                    .and_then(Value::as_u64)
                    .ok_or("length is required with offset")?;
                data.get(from as usize..from.checked_add(len).ok_or("that runs past the end")? as usize)
                    .ok_or_else(|| format!("{path} has {} bytes: {from:#x}+{len:#x} runs past the end", data.len()))?
                    .to_vec()
            }
            (Some(member), None) => {
                if !binviz::Container::is_container(&data) {
                    return Err(format!(
                        "{path} holds no files; give offset and length for a stretch of it"
                    ));
                }
                let c = binviz::Container::parse(data).map_err(|e| e.to_string())?;
                let index = c
                    .find(member)
                    .ok_or_else(|| format!("no member {member:?}; list_disc_files shows them"))?;
                c.member_data(index).map_err(|e| e.to_string())?.to_vec()
            }
            (None, None) => return Err("give member (a number or name) or offset and length".into()),
        };
        std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
        Ok(format!("Wrote {out} ({} bytes).", bytes.len()))
    }

    pub(crate) fn find_code_blobs(&mut self, args: &Value) -> Result<String, String> {
        let path = string(args, "path").ok_or("path is required")?;
        let data = read(path)?;
        let skip = match string(args, "psx_exe") {
            Some(e) => {
                let head = read(e)?;
                Some(binviz::blobs::exe_range(&head).ok_or_else(|| format!("{e} is not a PS-X EXE"))?)
            }
            None => None,
        };
        let mut blobs = binviz::blobs::find_code_blobs(&data, skip);
        let total = blobs.len();
        blobs.truncate(int(args, "limit", 200, 5000) as usize);
        let mut out = binviz::blobs::blobs_text(&blobs);
        if blobs.len() < total {
            out.push_str(&format!("({} more not shown)\n", total - blobs.len()));
        }
        out.push_str(
            "Open one as an overlay: extract_disc_file it (offset, length), then open_binary with overlay_at and psx_exe.\n",
        );
        Ok(out)
    }
}
