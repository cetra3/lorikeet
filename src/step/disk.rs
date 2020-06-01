use serde::{Deserialize, Serialize};
use std::cmp;

use log::*;
use std::{ffi::CString, mem::zeroed};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DiskVariant {
    MountPointOnly(String),
    Options(DiskOptions),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiskOptions {
    mount: String,
    #[serde(default, rename = "type")]
    disk_type: DiskType,
    #[serde(default)]
    output_type: OutputType,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskType {
    Size,
    Used,
    Free,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Bytes,
    Human,
    Percent,
}

impl Default for DiskType {
    fn default() -> Self {
        DiskType::Free
    }
}

impl Default for OutputType {
    fn default() -> Self {
        OutputType::Bytes
    }
}

impl DiskVariant {
    pub async fn run(&self) -> Result<String, String> {
        let diskops = match *self {
            DiskVariant::MountPointOnly(ref mount) => DiskOptions {
                mount: mount.clone(),
                disk_type: DiskType::Free,
                output_type: OutputType::Bytes,
            },
            DiskVariant::Options(ref ops) => ops.clone(),
        };

        let stavfs = get_stats(&diskops)?;

        return Ok(stavfs.to_string());
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_stats(ops: &DiskOptions) -> Result<String, String> {
    let mountp = CString::new(ops.mount.clone()).unwrap();
    let mnt_ptr = mountp.as_ptr();

    let stats = unsafe {
        let mut stats: libc::statvfs = zeroed();
        if libc::statvfs(mnt_ptr, &mut stats) != 0 {
            return Err(format!(
                "Unable to retrive stats of {}: {}",
                ops.mount,
                std::io::Error::last_os_error()
            ));
        }

        stats
    };

    debug!(
        "f_blocks:{}, f_bsize:{}, f_frsize:{}, f_bavail:{}, f_bfree:{}",
        stats.f_blocks, stats.f_bsize, stats.f_frsize, stats.f_bavail, stats.f_bfree
    );

    let size = stats.f_blocks * stats.f_frsize;
    let free = stats.f_bavail * stats.f_frsize;
    let used = size - free;

    debug!("size: {}, free:{}, used:{}", size, free, used);

    let output = match ops.disk_type {
        DiskType::Size => size,
        DiskType::Used => used,
        DiskType::Free => free,
    };

    match ops.output_type {
        OutputType::Bytes => return Ok(output.to_string()),
        OutputType::Percent => {
            if size == 0 {
                return Err(format!(
                    "Size for mount `{}` is 0.  Can't create percentage",
                    ops.mount
                ));
            }
            return Ok(format!(
                "{}%",
                ((output as f64 / size as f64) * 100.0).round() as usize
            ));
        }
        OutputType::Human => return Ok(pretty_bytes(output as f64)),
    }
}

#[cfg(target_os = "windows")]
pub fn get_stats(_ops: &DiskOptions) -> Result<u64, String> {
    return Err("Not Implemented Yet".into());
}

pub fn pretty_bytes(num: f64) -> String {
    let negative = if num.is_sign_positive() { "" } else { "-" };
    let num = num.abs();
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    if num < 1_f64 {
        return format!("{}{} {}", negative, num, "B");
    }
    let delimiter = 1000_f64;
    let exponent = cmp::min(
        (num.ln() / delimiter.ln()).floor() as i32,
        (units.len() - 1) as i32,
    );

    let unit = units[exponent as usize];
    return format!("{}{:.2}{}", negative, num / delimiter.powi(exponent), unit);
}
