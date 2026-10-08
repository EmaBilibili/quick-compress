mod image_ops;
mod video;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Button, ProgressBar, Spinner};
use libadwaita::prelude::*;
use libadwaita::{
    ActionRow, Application, ApplicationWindow, HeaderBar, PreferencesGroup, StatusPage,
    Toast, ToastOverlay, ViewStack,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread;

const APP_ID: &str = "io.github.EmaBilibili.QuickCompress";

#[derive(Clone)]
struct FileInfo {
    path: PathBuf,
    name: String,
    size_bytes: u64,
    is_video: bool,
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn is_supported_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext = ext.to_lowercase();
        matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "mp4" | "mkv" | "mov" | "webm" | "avi"
        )
    } else {
        false
    }
}

fn is_video_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext = ext.to_lowercase();
        matches!(ext.as_str(), "mp4" | "mkv" | "mov" | "webm" | "avi")
    } else {
        false
    }
}

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let header_bar = HeaderBar::new();
    let toast_overlay = ToastOverlay::new();

    let view_stack = ViewStack::new();

    // 1. Drop Zone (Initial View)
    let drop_status_page = StatusPage::builder()
        .icon_name("document-send-symbolic")
        .title("QuickCompress")
        .description("Drag and drop images, video or audio files here\nor choose a file to compress")
        .build();

    let open_file_btn = Button::builder()
        .label("Choose File…")
        .css_classes(["pill", "suggested-action"])
        .halign(gtk4::Align::Center)
        .build();

    let drop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    drop_box.append(&drop_status_page);
    drop_box.append(&open_file_btn);
    drop_box.set_valign(gtk4::Align::Center);

    view_stack.add_titled(&drop_box, Some("drop"), "Drop");

    // 2. File Selected & Compression Actions View
    let file_group = PreferencesGroup::builder()
        .title("Selected File")
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(24)
        .margin_end(24)
        .build();

    let file_row_icon = gtk4::Image::from_icon_name("image-x-generic-symbolic");
    let file_row = ActionRow::builder()
        .title("File Name")
        .subtitle("Size: -")
        .build();
    file_row.add_prefix(&file_row_icon);
    file_group.add(&file_row);

    // Actions Group
    let actions_group = PreferencesGroup::builder()
        .title("Actions")
        .margin_start(24)
        .margin_end(24)
        .build();

    // Video Action Buttons
    let opt_discord_btn = Button::with_label("Target < 10 MB (Discord / WhatsApp)");
    opt_discord_btn.add_css_class("suggested-action");

    let opt_email_btn = Button::with_label("Target < 25 MB (Email / Telegram)");

    // Image Action Buttons
    let opt_webp_lossy_btn = Button::with_label("Convert to WebP (Optimized / Lossy ~78%)");
    opt_webp_lossy_btn.add_css_class("suggested-action");

    let opt_webp_lossless_btn = Button::with_label("Convert to WebP (Lossless for PNGs/Art)");
    let opt_jpeg_chat_btn = Button::with_label("Compress JPEG (Max 1920px, 80% Quality)");
    let opt_resize_50_btn = Button::with_label("Scale Down 50% (Halve Dimensions)");
    let opt_strip_exif_btn = Button::with_label("Strip EXIF & Privacy Metadata (GPS/Camera)");

    // Progress bar and spinner
    let spinner = Spinner::new();
    spinner.set_halign(gtk4::Align::Center);
    spinner.set_visible(false);

    let progress_bar = ProgressBar::new();
    progress_bar.set_pulse_step(0.1);
    progress_bar.set_visible(false);

    let status_label = gtk4::Label::new(None);
    status_label.set_visible(false);
    status_label.add_css_class("dim-label");

    let reset_btn = Button::with_label("Choose another file");
    reset_btn.add_css_class("flat");

    let actions_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    actions_box.append(&opt_discord_btn);
    actions_box.append(&opt_email_btn);
    actions_box.append(&opt_webp_lossy_btn);
    actions_box.append(&opt_webp_lossless_btn);
    actions_box.append(&opt_jpeg_chat_btn);
    actions_box.append(&opt_resize_50_btn);
    actions_box.append(&opt_strip_exif_btn);
    actions_box.append(&spinner);
    actions_box.append(&progress_bar);
    actions_box.append(&status_label);
    actions_box.append(&reset_btn);
    actions_group.add(&actions_box);

    let details_container = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    details_container.append(&file_group);
    details_container.append(&actions_group);

    view_stack.add_titled(&details_container, Some("actions"), "Actions");
    view_stack.set_visible_child_name("drop");

    // Layout
    let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content_box.append(&header_bar);
    content_box.append(&view_stack);
    toast_overlay.set_child(Some(&content_box));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("QuickCompress")
        .default_width(520)
        .default_height(480)
        .content(&toast_overlay)
        .build();

    let current_file: Rc<RefCell<Option<FileInfo>>> = Rc::new(RefCell::new(None));

    // Helper: update UI on file load
    let update_file_selected = {
        let view_stack = view_stack.clone();
        let file_row = file_row.clone();
        let file_row_icon = file_row_icon.clone();
        let current_file = current_file.clone();
        let actions_group = actions_group.clone();
        let opt_discord_btn = opt_discord_btn.clone();
        let opt_email_btn = opt_email_btn.clone();
        let opt_webp_lossy_btn = opt_webp_lossy_btn.clone();
        let opt_webp_lossless_btn = opt_webp_lossless_btn.clone();
        let opt_jpeg_chat_btn = opt_jpeg_chat_btn.clone();
        let opt_resize_50_btn = opt_resize_50_btn.clone();
        let opt_strip_exif_btn = opt_strip_exif_btn.clone();

        move |path: PathBuf| {
            if let Ok(metadata) = fs::metadata(&path) {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let size = metadata.len();
                let is_video = is_video_file(&path);

                file_row.set_title(&name);
                file_row.set_subtitle(&format!("Original size: {}", format_bytes(size)));
                file_row_icon.set_icon_name(Some(if is_video {
                    "video-x-generic-symbolic"
                } else {
                    "image-x-generic-symbolic"
                }));

                // Adapt action buttons based on file type
                if is_video {
                    actions_group.set_title("Quick Video Compression");
                    opt_discord_btn.set_visible(true);
                    opt_email_btn.set_visible(true);
                    opt_webp_lossy_btn.set_visible(false);
                    opt_webp_lossless_btn.set_visible(false);
                    opt_jpeg_chat_btn.set_visible(false);
                    opt_resize_50_btn.set_visible(false);
                    opt_strip_exif_btn.set_visible(false);
                } else {
                    actions_group.set_title("Quick Image Actions");
                    opt_discord_btn.set_visible(false);
                    opt_email_btn.set_visible(false);
                    opt_webp_lossy_btn.set_visible(true);
                    opt_webp_lossless_btn.set_visible(true);
                    opt_jpeg_chat_btn.set_visible(true);
                    opt_resize_50_btn.set_visible(true);
                    opt_strip_exif_btn.set_visible(true);
                }

                *current_file.borrow_mut() = Some(FileInfo {
                    path,
                    name,
                    size_bytes: size,
                    is_video,
                });

                view_stack.set_visible_child_name("actions");
            }
        }
    };

    // Reset button
    {
        let view_stack = view_stack.clone();
        let current_file = current_file.clone();
        reset_btn.connect_clicked(move |_| {
            *current_file.borrow_mut() = None;
            view_stack.set_visible_child_name("drop");
        });
    }

    // Drag and Drop Target
    let drop_target = gtk4::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    {
        let update_file_selected = update_file_selected.clone();
        let toast_overlay = toast_overlay.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            if let Ok(file_list) = value.get::<gdk::FileList>() {
                let files = file_list.files();
                if let Some(first_file) = files.first() {
                    if let Some(path) = first_file.path() {
                        if is_supported_file(&path) {
                            update_file_selected(path);
                            return true;
                        } else {
                            toast_overlay.add_toast(Toast::new("Unsupported file format"));
                        }
                    }
                }
            }
            false
        });
    }
    window.add_controller(drop_target);

    // File Chooser Button Dialog
    {
        let window_clone = window.clone();
        let update_file_selected = update_file_selected.clone();
        open_file_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::new();
            dialog.set_title("Choose File to Compress");

            let update = update_file_selected.clone();
            dialog.open(Some(&window_clone), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        if is_supported_file(&path) {
                            update(path);
                        }
                    }
                }
            });
        });
    }

    // Helper for async video compression
    let trigger_video_compression = {
        let current_file = current_file.clone();
        let toast_overlay = toast_overlay.clone();
        let opt_discord_btn = opt_discord_btn.clone();
        let opt_email_btn = opt_email_btn.clone();
        let spinner = spinner.clone();
        let progress_bar = progress_bar.clone();
        let status_label = status_label.clone();
        let reset_btn = reset_btn.clone();

        move |target_mb: f64| {
            let file_opt = current_file.borrow().clone();
            if let Some(info) = file_opt {
                if !info.is_video {
                    toast_overlay.add_toast(Toast::new("Selected file is not a video"));
                    return;
                }

                // UI loading state
                opt_discord_btn.set_sensitive(false);
                opt_email_btn.set_sensitive(false);
                reset_btn.set_sensitive(false);
                spinner.set_visible(true);
                spinner.start();
                progress_bar.set_visible(true);
                status_label.set_visible(true);
                status_label.set_text(&format!("Compressing video to < {:.0} MB...", target_mb));

                let out_suffix = format!("{:.0}mb", target_mb);
                let out_path = video::generate_output_path(&info.path, &out_suffix, "mp4");

                let (sender, receiver) = async_channel::bounded::<Result<PathBuf, String>>(1);

                // Pulse timer
                let pbar = progress_bar.clone();
                let pulse_timer = glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                    pbar.pulse();
                    glib::ControlFlow::Continue
                });

                // Spawn worker thread
                let input_path = info.path.clone();
                let output_path = out_path.clone();
                thread::spawn(move || {
                    let res = video::compress_video_target_mb(&input_path, &output_path, target_mb)
                        .map(|_| output_path);
                    let _ = sender.send_blocking(res);
                });

                // Spawn local async handler on main thread to update UI
                let toast_overlay = toast_overlay.clone();
                let opt_discord = opt_discord_btn.clone();
                let opt_email = opt_email_btn.clone();
                let reset = reset_btn.clone();
                let spin = spinner.clone();
                let pbar = progress_bar.clone();
                let status_lbl = status_label.clone();

                glib::MainContext::default().spawn_local(async move {
                    if let Ok(res) = receiver.recv().await {
                        pulse_timer.remove();
                        spin.stop();
                        spin.set_visible(false);
                        pbar.set_visible(false);
                        status_lbl.set_visible(false);
                        opt_discord.set_sensitive(true);
                        opt_email.set_sensitive(true);
                        reset.set_sensitive(true);

                        match res {
                            Ok(saved_path) => {
                                let size_str = fs::metadata(&saved_path)
                                    .map(|m| format_bytes(m.len()))
                                    .unwrap_or_default();
                                let msg = format!(
                                    "Saved: {} ({})",
                                    saved_path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("video.mp4"),
                                    size_str
                                );
                                toast_overlay.add_toast(Toast::new(&msg));
                            }
                            Err(err) => {
                                toast_overlay.add_toast(Toast::new(&format!("Error: {}", err)));
                            }
                        }
                    }
                });
            }
        }
    };

    // Helper for async image compression
    let trigger_image_compression = {
        let current_file = current_file.clone();
        let toast_overlay = toast_overlay.clone();
        let opt_webp_lossy_btn = opt_webp_lossy_btn.clone();
        let opt_webp_lossless_btn = opt_webp_lossless_btn.clone();
        let opt_jpeg_chat_btn = opt_jpeg_chat_btn.clone();
        let opt_resize_50_btn = opt_resize_50_btn.clone();
        let opt_strip_exif_btn = opt_strip_exif_btn.clone();
        let spinner = spinner.clone();
        let progress_bar = progress_bar.clone();
        let status_label = status_label.clone();
        let reset_btn = reset_btn.clone();

        // mode: 0 = WebP Lossy, 1 = WebP Lossless, 2 = JPEG Chat, 3 = Scale 50%, 4 = Strip EXIF
        move |mode: u8| {
            let file_opt = current_file.borrow().clone();
            if let Some(info) = file_opt {
                if info.is_video {
                    toast_overlay.add_toast(Toast::new("Selected file is not an image"));
                    return;
                }

                // UI loading state
                opt_webp_lossy_btn.set_sensitive(false);
                opt_webp_lossless_btn.set_sensitive(false);
                opt_jpeg_chat_btn.set_sensitive(false);
                opt_resize_50_btn.set_sensitive(false);
                opt_strip_exif_btn.set_sensitive(false);
                reset_btn.set_sensitive(false);
                spinner.set_visible(true);
                spinner.start();
                progress_bar.set_visible(true);
                status_label.set_visible(true);
                status_label.set_text(match mode {
                    0 => "Optimizing to WebP (Lossy ~78%)...",
                    1 => "Converting to WebP (Lossless)...",
                    2 => "Compressing JPEG for chat...",
                    3 => "Scaling image down 50%...",
                    _ => "Stripping EXIF & GPS metadata...",
                });

                let orig_ext = info
                    .path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("png");

                let (out_suffix, ext) = match mode {
                    0 => ("optimized", "webp"),
                    1 => ("lossless", "webp"),
                    2 => ("chat", "jpg"),
                    3 => ("scaled50", orig_ext),
                    _ => ("clean", orig_ext),
                };
                let out_path = image_ops::generate_image_output_path(&info.path, out_suffix, ext);

                let (sender, receiver) = async_channel::bounded::<Result<PathBuf, String>>(1);

                // Pulse timer
                let pbar = progress_bar.clone();
                let pulse_timer = glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
                    pbar.pulse();
                    glib::ControlFlow::Continue
                });

                // Spawn worker thread
                let input_path = info.path.clone();
                let output_path = out_path.clone();
                thread::spawn(move || {
                    let res = match mode {
                        0 => image_ops::convert_to_webp(&input_path, &output_path, false),
                        1 => image_ops::convert_to_webp(&input_path, &output_path, true),
                        2 => image_ops::optimize_jpeg(&input_path, &output_path, 1920),
                        3 => image_ops::resize_image(&input_path, &output_path, Some(0.5), None),
                        _ => image_ops::strip_metadata(&input_path, &output_path),
                    }
                    .map(|_| output_path);

                    let _ = sender.send_blocking(res);
                });

                // Spawn local async handler on main thread to update UI
                let toast_overlay = toast_overlay.clone();
                let opt_webp_lossy = opt_webp_lossy_btn.clone();
                let opt_webp_lossless = opt_webp_lossless_btn.clone();
                let opt_jpeg = opt_jpeg_chat_btn.clone();
                let opt_resize = opt_resize_50_btn.clone();
                let opt_strip = opt_strip_exif_btn.clone();
                let reset = reset_btn.clone();
                let spin = spinner.clone();
                let pbar = progress_bar.clone();
                let status_lbl = status_label.clone();

                glib::MainContext::default().spawn_local(async move {
                    if let Ok(res) = receiver.recv().await {
                        pulse_timer.remove();
                        spin.stop();
                        spin.set_visible(false);
                        pbar.set_visible(false);
                        status_lbl.set_visible(false);
                        opt_webp_lossy.set_sensitive(true);
                        opt_webp_lossless.set_sensitive(true);
                        opt_jpeg.set_sensitive(true);
                        opt_resize.set_sensitive(true);
                        opt_strip.set_sensitive(true);
                        reset.set_sensitive(true);

                        match res {
                            Ok(saved_path) => {
                                let orig_size = info.size_bytes;
                                let new_size = fs::metadata(&saved_path)
                                    .map(|m| m.len())
                                    .unwrap_or(0);
                                let saved_pct = if orig_size > new_size && orig_size > 0 {
                                    ((orig_size - new_size) as f64 / orig_size as f64) * 100.0
                                } else {
                                    0.0
                                };

                                let msg = if saved_pct > 0.0 {
                                    format!(
                                        "Saved: {} ({}, -{:.0}%)",
                                        saved_path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("image"),
                                        format_bytes(new_size),
                                        saved_pct
                                    )
                                } else {
                                    format!(
                                        "Saved: {} ({})",
                                        saved_path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("image"),
                                        format_bytes(new_size)
                                    )
                                };
                                toast_overlay.add_toast(Toast::new(&msg));
                            }
                            Err(err) => {
                                toast_overlay.add_toast(Toast::new(&format!("Error: {}", err)));
                            }
                        }
                    }
                });
            }
        }
    };

    // Connect video compression buttons
    {
        let trigger = trigger_video_compression.clone();
        opt_discord_btn.connect_clicked(move |_| {
            trigger(9.5); // Discord 10 MB limit safe target
        });
    }

    {
        let trigger = trigger_video_compression;
        opt_email_btn.connect_clicked(move |_| {
            trigger(24.0); // 25 MB email/telegram safe target
        });
    }

    // Connect image compression buttons
    {
        let trigger = trigger_image_compression.clone();
        opt_webp_lossy_btn.connect_clicked(move |_| {
            trigger(0); // WebP Lossy (Optimized)
        });
    }

    {
        let trigger = trigger_image_compression.clone();
        opt_webp_lossless_btn.connect_clicked(move |_| {
            trigger(1); // WebP Lossless
        });
    }

    {
        let trigger = trigger_image_compression.clone();
        opt_jpeg_chat_btn.connect_clicked(move |_| {
            trigger(2); // JPEG Chat
        });
    }

    {
        let trigger = trigger_image_compression.clone();
        opt_resize_50_btn.connect_clicked(move |_| {
            trigger(3); // Resize 50%
        });
    }

    {
        let trigger = trigger_image_compression;
        opt_strip_exif_btn.connect_clicked(move |_| {
            trigger(4); // Strip EXIF & GPS
        });
    }

    window.present();
}
