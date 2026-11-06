//! Whether a TV is connected, from the DRM connector status the kernel keeps in sysfs.

use std::path::Path;

const DRM: &str = "/sys/class/drm";

pub fn connected() -> bool {
    connected_in(Path::new(DRM))
}

pub fn connected_in(drm: &Path) -> bool {
    std::fs::read_dir(drm)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("-HDMI-"))
        .any(|e| {
            std::fs::read_to_string(e.path().join("status")).is_ok_and(|s| s.trim() == "connected")
        })
}
