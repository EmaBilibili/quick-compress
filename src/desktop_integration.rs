use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SVG_ICON_DATA: &[u8] = include_bytes!("../data/icons/io.github.EmaBilibili.QuickCompress.svg");
const PNG_16_DATA: &[u8] = include_bytes!("../data/icons/hicolor/16x16/apps/io.github.EmaBilibili.QuickCompress.png");
const PNG_24_DATA: &[u8] = include_bytes!("../data/icons/hicolor/24x24/apps/io.github.EmaBilibili.QuickCompress.png");
const PNG_32_DATA: &[u8] = include_bytes!("../data/icons/hicolor/32x32/apps/io.github.EmaBilibili.QuickCompress.png");
const PNG_48_DATA: &[u8] = include_bytes!("../data/icons/hicolor/48x48/apps/io.github.EmaBilibili.QuickCompress.png");
const PNG_64_DATA: &[u8] = include_bytes!("../data/icons/hicolor/64x64/apps/io.github.EmaBilibili.QuickCompress.png");
const PNG_128_DATA: &[u8] = include_bytes!("../data/icons/hicolor/128x128/apps/io.github.EmaBilibili.QuickCompress.png");
const PNG_256_DATA: &[u8] = include_bytes!("../data/icons/hicolor/256x256/apps/io.github.EmaBilibili.QuickCompress.png");
const DESKTOP_ENTRY_DATA: &[u8] = include_bytes!("../data/io.github.EmaBilibili.QuickCompress.desktop");

/// Auto-installs desktop launcher and multi-size icons for the current user upon first run.
/// This guarantees any user who downloads the bare executable gets the icon and menu integration.
pub fn ensure_desktop_integration() {
    let Some(home_dir) = env::var_os("HOME").map(PathBuf::from) else {
        return;
    };

    let apps_dir = home_dir.join(".local/share/applications");
    let icons_base = home_dir.join(".local/share/icons/hicolor");
    let desktop_file = apps_dir.join("io.github.EmaBilibili.QuickCompress.desktop");

    // Only install if not present or updated
    if !desktop_file.exists() {
        let _ = fs::create_dir_all(&apps_dir);
        let _ = fs::write(&desktop_file, DESKTOP_ENTRY_DATA);

        // Install Scalable SVG
        let svg_dir = icons_base.join("scalable/apps");
        let _ = fs::create_dir_all(&svg_dir);
        let _ = fs::write(svg_dir.join("io.github.EmaBilibili.QuickCompress.svg"), SVG_ICON_DATA);
        let _ = fs::write(svg_dir.join("quick-compress.svg"), SVG_ICON_DATA);

        // Install PNG sizes
        let png_sizes: &[(&str, &[u8])] = &[
            ("16x16", PNG_16_DATA),
            ("24x24", PNG_24_DATA),
            ("32x32", PNG_32_DATA),
            ("48x48", PNG_48_DATA),
            ("64x64", PNG_64_DATA),
            ("128x128", PNG_128_DATA),
            ("256x256", PNG_256_DATA),
        ];

        for (size, data) in png_sizes {
            let dir = icons_base.join(size).join("apps");
            let _ = fs::create_dir_all(&dir);
            let _ = fs::write(dir.join("io.github.EmaBilibili.QuickCompress.png"), *data);
            let _ = fs::write(dir.join("quick-compress.png"), *data);
        }

        // Refresh system caches quietly in background
        let _ = Command::new("gtk-update-icon-cache").args(["-f", "-t"]).arg(&icons_base).spawn();
        let _ = Command::new("kbuildsycoca6").spawn();
        let _ = Command::new("update-desktop-database").arg(&apps_dir).spawn();
    }
}
