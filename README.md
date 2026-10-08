# QuickCompress

<div align="center">
  <h3>⚡ Fast, native Linux utility to compress and convert images, audio, and video for chat and web.</h3>
</div>

---

## ✨ Features (Roadmap)

- 🖼️ **Image Compression & Conversion:** PNG, JPEG, WebP, HEIC/AVIF.
- 🎬 **Video Optimization:** Target custom file size (e.g., Discord/WhatsApp limits: `< 10 MB`, `< 25 MB`).
- 🎵 **Audio Extraction & Compression:** Fast conversion to MP3/OGG/AAC.
- 📋 **Clipboard Direct Support:** Paste an image directly or copy the compressed output without saving to disk.
- 🎨 **Modern Native UI:** Built with GTK4 and Libadwaita following GNOME HIG, adaptive layout and dark mode support.
- 🔒 **100% Local & Private:** No cloud, no telemetry, all processing happens on your machine.

---

## 🛠️ Tech Stack

- **Language:** [Rust](https://www.rust-lang.org/)
- **UI Toolkit:** [GTK4](https://gtk.org/) & [Libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)
- **Backend:** `image`, `oxipng`, `ffmpeg`

---

## 🚀 Building from Source

### Prerequisites

Ensure you have Rust and the development headers installed.

**Arch / CachyOS:**
```bash
sudo pacman -S rust gtk4 libadwaita ffmpeg
```

**Fedora:**
```bash
sudo dnf install rust cargo gtk4-devel libadwaita-devel ffmpeg-free-devel
```

**Ubuntu / Debian:**
```bash
sudo apt install cargo libgtk-4-dev libadwaita-1-dev ffmpeg
```

### Run in Development

```bash
cargo run
```

---

## 📜 License

Licensed under the [GNU General Public License v3.0](LICENSE).
