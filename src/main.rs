use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::Button;
use libadwaita::prelude::*;
use libadwaita::{
    ActionRow, Application, ApplicationWindow, HeaderBar, PreferencesGroup, StatusPage,
    Toast, ToastOverlay, ViewStack,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

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
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "mp4" | "mkv" | "mov" | "webm" | "mp3" | "wav" | "ogg"
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

    let actions_group = PreferencesGroup::builder()
        .title("Quick Actions")
        .margin_start(24)
        .margin_end(24)
        .build();

    let opt_chat_btn = Button::with_label("Compress for Chat (< 10 MB)");
    opt_chat_btn.add_css_class("suggested-action");

    let opt_webp_btn = Button::with_label("Convert to WebP (High Efficiency)");
    let reset_btn = Button::with_label("Choose another file");
    reset_btn.add_css_class("flat");

    let actions_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    actions_box.append(&opt_chat_btn);
    actions_box.append(&opt_webp_btn);
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
        .default_height(460)
        .content(&toast_overlay)
        .build();

    let current_file: Rc<RefCell<Option<FileInfo>>> = Rc::new(RefCell::new(None));

    // Helper: update UI on file load
    let update_file_selected = {
        let view_stack = view_stack.clone();
        let file_row = file_row.clone();
        let file_row_icon = file_row_icon.clone();
        let current_file = current_file.clone();
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

    // Action Toast placeholders
    {
        let toast_overlay = toast_overlay.clone();
        opt_chat_btn.connect_clicked(move |_| {
            toast_overlay.add_toast(Toast::new("Compressing for chat... (Processing)"));
        });
    }

    {
        let toast_overlay = toast_overlay.clone();
        opt_webp_btn.connect_clicked(move |_| {
            toast_overlay.add_toast(Toast::new("Converting to WebP... (Processing)"));
        });
    }

    window.present();
}
