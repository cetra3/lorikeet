use serde::{Deserialize, Serialize};

use lazy_static::lazy_static;
use sys_info::{disk_info, loadavg, mem_info};
use tokio::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemVariant {
    MemTotal,
    MemFree,
    MemAvailable,
    LoadAvg1m,
    LoadAvg5m,
    LoadAvg15m,
    DiskTotal,
    DiskFree,
}

lazy_static! {
    static ref SYS_MUTEX: Mutex<()> = Mutex::new(());
}

impl SystemVariant {
    pub async fn run(&self) -> Result<String, String> {
        // This is a workaround for a memory bug in `sys_info`
        // See: https://github.com/FillZpp/sys-info-rs/issues/63
        let _guard = SYS_MUTEX.lock().await;
        match self {
            SystemVariant::LoadAvg1m => loadavg()
                .map(|load| load.one.to_string())
                .map_err(|_| "Could not get load".to_string()),
            SystemVariant::LoadAvg5m => loadavg()
                .map(|load| load.five.to_string())
                .map_err(|_| "Could not get load".to_string()),
            SystemVariant::LoadAvg15m => loadavg()
                .map(|load| load.fifteen.to_string())
                .map_err(|_| "Could not get load".to_string()),
            SystemVariant::MemAvailable => mem_info()
                .map(|mem| mem.avail.to_string())
                .map_err(|_| "Could not get memory".to_string()),
            SystemVariant::MemFree => mem_info()
                .map(|mem| mem.free.to_string())
                .map_err(|_| "Could not get memory".to_string()),
            SystemVariant::MemTotal => mem_info()
                .map(|mem| mem.total.to_string())
                .map_err(|_| "Could not get memory".to_string()),
            SystemVariant::DiskTotal => disk_info()
                .map(|disk| disk.total.to_string())
                .map_err(|_| "Could not get disk".to_string()),
            SystemVariant::DiskFree => disk_info()
                .map(|disk| disk.free.to_string())
                .map_err(|_| "Could not get disk".to_string()),
        }
    }
}
