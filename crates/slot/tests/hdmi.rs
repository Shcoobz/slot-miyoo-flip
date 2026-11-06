use std::fs;

#[test]
fn a_tv_counts_as_connected_only_when_its_hdmi_connector_says_so() {
    let d = tempfile::tempdir().unwrap();
    let drm = d.path();
    let connector = |name: &str, status: &str| {
        fs::create_dir_all(drm.join(name)).unwrap();
        fs::write(drm.join(name).join("status"), format!("{status}\n")).unwrap();
    };
    connector("card0-DSI-1", "connected");
    connector("card0-HDMI-A-1", "disconnected");
    assert!(!slot::hdmi::connected_in(drm));
    connector("card0-HDMI-A-1", "connected");
    assert!(slot::hdmi::connected_in(drm));
}
