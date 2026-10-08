mod desktop_integration;
mod image_ops;
mod updater;
mod video;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Button, Entry, EventControllerKey, Label, Picture, ProgressBar, Scale, ScrolledWindow, Spinner};
use libadwaita::prelude::*;
use libadwaita::{
    ActionRow, Application, ApplicationWindow, Clamp, HeaderBar, MessageDialog, PreferencesGroup,
    PreferencesPage, PreferencesWindow, StatusPage, Toast, ToastOverlay, ViewStack,
};
use std::cell::RefCell;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const APP_ID: &str = "io.github.EmaBilibili.QuickCompress";

struct ActivePlayback {
    audio_child: Option<Child>,
    video_child: Option<Child>,
    stop_flag: Arc<AtomicBool>,
}

#[derive(Clone)]
struct FileItem {
    path: PathBuf,
    name: String,
    size_bytes: u64,
    is_video: bool,
}

#[derive(Clone, Default)]
struct AppConfig {
    custom_save_dir: Option<PathBuf>,
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

fn format_time_display(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{:02}:{:02}", mins, secs)
}

fn parse_time_str(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 2 {
            let mins: f64 = parts[0].parse().ok()?;
            let secs: f64 = parts[1].parse().ok()?;
            return Some(mins * 60.0 + secs);
        }
    }

    s.parse::<f64>().ok()
}

static FRAME_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn get_next_preview_frame_path() -> PathBuf {
    let id = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed) % 4;
    env::temp_dir().join(format!("quickcompress_preview_{}.jpg", id))
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

fn open_folder_containing(path: &Path) {
    let target = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or_else(|| Path::new("."))
    };

    let _ = Command::new("xdg-open").arg(target).spawn();
}

fn show_preferences_window(parent: &ApplicationWindow, config: Rc<RefCell<AppConfig>>) {
    let prefs = PreferencesWindow::builder()
        .transient_for(parent)
        .modal(true)
        .title("Preferences")
        .default_width(480)
        .default_height(420)
        .build();

    let page = PreferencesPage::new();

    // Destination Folder Group
    let dest_group = PreferencesGroup::builder()
        .title("Output Directory")
        .description("Choose where compressed files are saved")
        .build();

    let dest_row = ActionRow::builder()
        .title("Save Location")
        .subtitle(
            config
                .borrow()
                .custom_save_dir
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Same folder as original file".to_string()),
        )
        .build();

    let change_dest_btn = Button::with_label("Select Folder");
    change_dest_btn.add_css_class("flat");

    let reset_dest_btn = Button::with_label("Reset to Default");
    reset_dest_btn.add_css_class("flat");

    let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    btn_box.append(&change_dest_btn);
    btn_box.append(&reset_dest_btn);
    dest_row.add_suffix(&btn_box);
    dest_group.add(&dest_row);

    // Folder selection dialog
    {
        let parent_clone = parent.clone();
        let config_clone = config.clone();
        let dest_row_clone = dest_row.clone();

        change_dest_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::new();
            dialog.set_title("Select Output Folder");

            let config_clone = config_clone.clone();
            let dest_row_clone = dest_row_clone.clone();

            dialog.select_folder(Some(&parent_clone), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        dest_row_clone.set_subtitle(&path.to_string_lossy());
                        config_clone.borrow_mut().custom_save_dir = Some(path);
                    }
                }
            });
        });
    }

    {
        let config_clone = config.clone();
        let dest_row_clone = dest_row;
        reset_dest_btn.connect_clicked(move |_| {
            config_clone.borrow_mut().custom_save_dir = None;
            dest_row_clone.set_subtitle("Same folder as original file");
        });
    }

    // General Group
    let general_group = PreferencesGroup::builder()
        .title("General")
        .description("App version and information")
        .build();

    let version_row = ActionRow::builder()
        .title("Version")
        .subtitle(updater::CURRENT_VERSION)
        .build();
    general_group.add(&version_row);

    // Updates Group
    let updates_group = PreferencesGroup::builder()
        .title("Updates")
        .description("Keep QuickCompress up to date")
        .build();

    let check_updates_row = ActionRow::builder()
        .title("Check for Updates")
        .subtitle("Verify if a new version is published on GitHub")
        .activatable(true)
        .build();

    let check_btn = Button::with_label("Check Now");
    check_btn.add_css_class("flat");
    check_updates_row.add_suffix(&check_btn);
    updates_group.add(&check_updates_row);

    let parent_clone = parent.clone();
    check_btn.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        let parent = parent_clone.clone();
        let btn_clone = btn.clone();

        let (sender, receiver) = async_channel::bounded::<Result<updater::UpdateCheckResult, String>>(1);

        thread::spawn(move || {
            let res = updater::check_for_updates();
            let _ = sender.send_blocking(res);
        });

        glib::MainContext::default().spawn_local(async move {
            if let Ok(res) = receiver.recv().await {
                btn_clone.set_sensitive(true);
                match res {
                    Ok(info) if info.has_update => {
                        prompt_update_dialog(&parent, &info);
                    }
                    Ok(_) => {
                        let dialog = MessageDialog::builder()
                            .transient_for(&parent)
                            .heading("You're Up to Date!")
                            .body(&format!(
                                "QuickCompress v{} is the latest version available.",
                                updater::CURRENT_VERSION
                            ))
                            .build();
                        dialog.add_response("ok", "OK");
                        dialog.present();
                    }
                    Err(e) => {
                        let dialog = MessageDialog::builder()
                            .transient_for(&parent)
                            .heading("Check Failed")
                            .body(&e)
                            .build();
                        dialog.add_response("ok", "OK");
                        dialog.present();
                    }
                }
            }
        });
    });

    page.add(&dest_group);
    page.add(&general_group);
    page.add(&updates_group);
    prefs.add(&page);
    prefs.present();
}

fn prompt_update_dialog(parent: &ApplicationWindow, info: &updater::UpdateCheckResult) {
    let dialog = MessageDialog::builder()
        .transient_for(parent)
        .heading(&format!("Update Available: v{}", info.latest_version))
        .body(&format!(
            "A newer version of QuickCompress was released.\n\nRelease notes:\n{}\n\nWould you like to update now?",
            info.release_notes
        ))
        .build();

    dialog.add_response("cancel", "Not Now");
    dialog.add_response("update", "Update & Restart");
    dialog.set_response_appearance("update", libadwaita::ResponseAppearance::Suggested);

    let info_clone = info.clone();
    let parent_clone = parent.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "update" {
            if let Some(url) = &info_clone.download_url {
                let download_url = url.clone();
                let parent = parent_clone.clone();

                let loading_dialog = MessageDialog::builder()
                    .transient_for(&parent)
                    .heading("Updating QuickCompress...")
                    .body("Downloading new version and preparing restart. Please wait...")
                    .build();
                loading_dialog.present();

                let (sender, receiver) = async_channel::bounded::<Result<(), String>>(1);

                thread::spawn(move || {
                    let res = updater::perform_self_update_and_restart(&download_url);
                    let _ = sender.send_blocking(res);
                });

                glib::MainContext::default().spawn_local(async move {
                    if let Ok(Err(e)) = receiver.recv().await {
                        loading_dialog.close();
                        let err_dialog = MessageDialog::builder()
                            .heading("Update Failed")
                            .body(&e)
                            .build();
                        err_dialog.add_response("ok", "OK");
                        err_dialog.present();
                    }
                });
            } else {
                let _ = Command::new("xdg-open").arg(&info_clone.release_url).spawn();
            }
        }
    });

    dialog.present();
}

fn main() {
    desktop_integration::ensure_desktop_integration();

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let header_bar = HeaderBar::new();
    let toast_overlay = ToastOverlay::new();

    let app_config: Rc<RefCell<AppConfig>> = Rc::new(RefCell::new(AppConfig::default()));

    // Preferences button in header bar
    let prefs_btn = Button::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Settings & Preferences")
        .build();
    header_bar.pack_end(&prefs_btn);

    // Clear / Cancel button in header bar
    let clear_header_btn = Button::builder()
        .icon_name("edit-clear-all-symbolic")
        .tooltip_text("Clear Selection")
        .visible(false)
        .build();
    header_bar.pack_start(&clear_header_btn);

    let view_stack = ViewStack::new();

    // 1. Drop Zone (Initial View)
    let drop_status_page = StatusPage::builder()
        .icon_name("document-send-symbolic")
        .title("QuickCompress")
        .description("Drag and drop single or multiple files here\nor paste an image from clipboard (Ctrl+V)")
        .build();

    let open_file_btn = Button::builder()
        .label("Choose Files…")
        .css_classes(["pill", "suggested-action"])
        .halign(gtk4::Align::Center)
        .build();

    let paste_btn = Button::builder()
        .label("Paste from Clipboard")
        .css_classes(["pill", "flat"])
        .halign(gtk4::Align::Center)
        .build();

    let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    btn_box.set_halign(gtk4::Align::Center);
    btn_box.append(&open_file_btn);
    btn_box.append(&paste_btn);

    let drop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    drop_box.append(&drop_status_page);
    drop_box.append(&btn_box);
    drop_box.set_valign(gtk4::Align::Center);

    view_stack.add_titled(&drop_box, Some("drop"), "Drop");

    // 2. File Selected & Compression Actions View
    // Image thumbnail preview widget
    let preview_picture = Picture::builder()
        .can_shrink(true)
        .content_fit(gtk4::ContentFit::Contain)
        .height_request(160)
        .margin_top(8)
        .margin_bottom(8)
        .visible(false)
        .build();

    let file_group = PreferencesGroup::builder()
        .title("Selected Files")
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

    // Video Player & Trimming Group (Hidden for images)
    let video_preview_picture = Picture::builder()
        .can_shrink(true)
        .content_fit(gtk4::ContentFit::Contain)
        .height_request(240)
        .margin_top(8)
        .margin_bottom(8)
        .visible(false)
        .build();

    let trim_group = PreferencesGroup::builder()
        .title("Video Frame Preview & Trim")
        .description("Drag the slider to scrub through the video frames, or preview playback with external player")
        .margin_start(24)
        .margin_end(24)
        .visible(false)
        .build();

    let timeline_scale = Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 100.0, 0.1);
    timeline_scale.set_hexpand(true);
    timeline_scale.set_draw_value(false);

    let time_pos_label = Label::new(Some("00:00 / 00:00"));
    time_pos_label.add_css_class("dim-label");

    let timeline_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    timeline_box.set_margin_bottom(8);
    timeline_box.append(&timeline_scale);
    timeline_box.append(&time_pos_label);
    trim_group.add(&video_preview_picture);
    trim_group.add(&timeline_box);

    let play_preview_btn = Button::builder()
        .label("Play")
        .tooltip_text("Play or pause video directly in preview")
        .icon_name("media-playback-start-symbolic")
        .css_classes(["pill", "flat"])
        .build();

    let set_start_btn = Button::builder()
        .label("Set as Start")
        .icon_name("media-seek-backward-symbolic")
        .css_classes(["pill", "flat"])
        .build();

    let set_end_btn = Button::builder()
        .label("Set as End")
        .icon_name("media-seek-forward-symbolic")
        .css_classes(["pill", "flat"])
        .build();

    let clear_trim_btn = Button::builder()
        .label("Reset Trim")
        .icon_name("edit-undo-symbolic")
        .css_classes(["pill", "flat"])
        .build();

    let trim_buttons_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    trim_buttons_box.set_halign(gtk4::Align::Center);
    trim_buttons_box.append(&play_preview_btn);
    trim_buttons_box.append(&set_start_btn);
    trim_buttons_box.append(&set_end_btn);
    trim_buttons_box.append(&clear_trim_btn);
    trim_group.add(&trim_buttons_box);

    let trim_start_entry = Entry::builder()
        .placeholder_text("Start (00:00)")
        .hexpand(true)
        .build();

    let trim_end_entry = Entry::builder()
        .placeholder_text("End (00:00)")
        .hexpand(true)
        .build();

    let trim_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    trim_box.set_margin_top(8);
    trim_box.append(&trim_start_entry);
    trim_box.append(&trim_end_entry);
    trim_group.add(&trim_box);

    let current_video_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let video_duration_state: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));
    let is_updating_scale = Rc::new(RefCell::new(false));

    let active_playback: Rc<RefCell<Option<ActivePlayback>>> = Rc::new(RefCell::new(None));

    // When user drags timeline slider, extract and show the frame at that timestamp
    {
        let video_preview_picture = video_preview_picture.clone();
        let current_video_path = current_video_path.clone();
        let time_pos_label = time_pos_label.clone();
        let is_updating_scale = is_updating_scale.clone();
        let duration_state = video_duration_state.clone();
        let active_playback = active_playback.clone();
        let play_preview_btn_for_scale = play_preview_btn.clone();

        timeline_scale.connect_value_changed(move |scale| {
            if *is_updating_scale.borrow() {
                return;
            }

            // Stop any ongoing playback when manually dragging slider
            if let Some(mut active) = active_playback.borrow_mut().take() {
                active.stop_flag.store(true, Ordering::SeqCst);
                if let Some(mut child) = active.audio_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                if let Some(mut child) = active.video_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                play_preview_btn_for_scale.set_label("Play");
                play_preview_btn_for_scale.set_icon_name("media-playback-start-symbolic");
            }

            let target_sec = scale.value();
            let total_dur = *duration_state.borrow();
            time_pos_label.set_text(&format!(
                "{} / {}",
                format_time_display(target_sec),
                format_time_display(total_dur)
            ));

            if let Some(vid_path) = current_video_path.borrow().clone() {
                let temp_frame = get_next_preview_frame_path();
                let (sender, receiver) = async_channel::bounded::<PathBuf>(1);
                let pic = video_preview_picture.clone();

                thread::spawn(move || {
                    if video::extract_video_frame(&vid_path, target_sec, &temp_frame).is_ok() {
                        let _ = sender.send_blocking(temp_frame);
                    }
                });

                glib::MainContext::default().spawn_local(async move {
                    if let Ok(frame_path) = receiver.recv().await {
                        pic.set_filename(Some(&frame_path));
                        pic.set_visible(true);
                    }
                });
            }
        });
    }

    // Play/Pause embedded video and audio directly in previewer
    {
        let current_video_path = current_video_path.clone();
        let active_playback = active_playback.clone();
        let play_preview_btn_clone = play_preview_btn.clone();
        let timeline_scale = timeline_scale.clone();
        let trim_start_entry = trim_start_entry.clone();
        let trim_end_entry = trim_end_entry.clone();
        let video_preview_picture = video_preview_picture.clone();
        let time_pos_label = time_pos_label.clone();
        let video_duration_state = video_duration_state.clone();
        let is_updating_scale = is_updating_scale.clone();

        play_preview_btn.connect_clicked(move |_| {
            // If already playing, pause / stop it
            if let Some(mut active) = active_playback.borrow_mut().take() {
                active.stop_flag.store(true, Ordering::SeqCst);
                if let Some(mut child) = active.audio_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                if let Some(mut child) = active.video_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                play_preview_btn_clone.set_label("Play");
                play_preview_btn_clone.set_icon_name("media-playback-start-symbolic");
                return;
            }

            if let Some(vid_path) = current_video_path.borrow().clone() {
                let current_slider = timeline_scale.value();
                let trim_start_val = parse_time_str(&trim_start_entry.text());
                let trim_end_val = parse_time_str(&trim_end_entry.text());

                let mut start_val = trim_start_val.unwrap_or(current_slider);
                if let Some(end) = trim_end_val {
                    if current_slider >= end {
                        start_val = trim_start_val.unwrap_or(0.0);
                    } else if current_slider > start_val {
                        start_val = current_slider;
                    }
                } else if current_slider > start_val {
                    start_val = current_slider;
                }

                let duration_opt = trim_end_val.filter(|&end| end > start_val).map(|end| end - start_val);

                // Invisible background ffplay for audio
                let mut audio_cmd = Command::new("ffplay");
                audio_cmd.args([
                    "-nodisp",
                    "-autoexit",
                    "-ss",
                    &format!("{:.2}", start_val),
                ]);
                if let Some(dur) = duration_opt {
                    audio_cmd.args(["-t", &format!("{:.2}", dur)]);
                }
                audio_cmd.arg(&vid_path);
                audio_cmd.stdout(Stdio::null());
                audio_cmd.stderr(Stdio::null());
                let audio_child = audio_cmd.spawn().ok();

                // Video stream decoder piping MJPEG frames directly to the GTK Picture
                let mut video_cmd = Command::new("ffmpeg");
                video_cmd.args([
                    "-ss",
                    &format!("{:.2}", start_val),
                    "-i",
                ]);
                video_cmd.arg(&vid_path);
                if let Some(dur) = duration_opt {
                    video_cmd.args(["-t", &format!("{:.2}", dur)]);
                }
                video_cmd.args([
                    "-vf",
                    "scale='min(640,iw)':-2,fps=24",
                    "-c:v",
                    "mjpeg",
                    "-f",
                    "image2pipe",
                    "pipe:1",
                ]);
                video_cmd.stdout(Stdio::piped());
                video_cmd.stderr(Stdio::null());

                let mut video_child = match video_cmd.spawn() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let mut stdout = match video_child.stdout.take() {
                    Some(s) => s,
                    None => return,
                };

                let stop_flag = Arc::new(AtomicBool::new(false));
                let stop_flag_clone = stop_flag.clone();
                let (frame_sender, frame_receiver) = async_channel::bounded::<(Vec<u8>, f64)>(4);

                thread::spawn(move || {
                    let frame_interval = Duration::from_micros(1_000_000 / 24);
                    let start_instant = Instant::now();
                    let mut frame_count: u64 = 0;
                    let mut buf = Vec::with_capacity(64 * 1024);
                    let mut chunk = [0u8; 8192];

                    while !stop_flag_clone.load(Ordering::Relaxed) {
                        let n = match stdout.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };
                        buf.extend_from_slice(&chunk[..n]);

                        while buf.len() >= 4 {
                            let soi = match buf.windows(2).position(|w| w == [0xFF, 0xD8]) {
                                Some(pos) => pos,
                                None => {
                                    buf.clear();
                                    break;
                                }
                            };

                            let eoi = match buf[soi + 2..].windows(2).position(|w| w == [0xFF, 0xD9]) {
                                Some(pos) => soi + 2 + pos + 2,
                                None => {
                                    if soi > 0 {
                                        buf.drain(..soi);
                                    }
                                    break;
                                }
                            };

                            let frame_bytes = buf[soi..eoi].to_vec();
                            buf.drain(..eoi);

                            frame_count += 1;
                            let current_pts = start_val + (frame_count as f64 / 24.0);
                            let target_time = start_instant + frame_interval * frame_count as u32;
                            let now = Instant::now();
                            if target_time > now {
                                thread::sleep(target_time - now);
                            }

                            if frame_sender.send_blocking((frame_bytes, current_pts)).is_err() {
                                return;
                            }
                        }
                    }
                });

                let pic = video_preview_picture.clone();
                let scale = timeline_scale.clone();
                let label = time_pos_label.clone();
                let total_dur = *video_duration_state.borrow();
                let is_updating = is_updating_scale.clone();
                let btn = play_preview_btn_clone.clone();
                let active_playback_for_recv = active_playback.clone();

                glib::MainContext::default().spawn_local(async move {
                    while let Ok((bytes, current_pts)) = frame_receiver.recv().await {
                        let glib_bytes = glib::Bytes::from(&bytes);
                        if let Ok(texture) = gtk4::gdk::Texture::from_bytes(&glib_bytes) {
                            pic.set_paintable(Some(&texture));
                            pic.set_visible(true);
                        }

                        *is_updating.borrow_mut() = true;
                        let clamped_pts = current_pts.min(total_dur);
                        scale.set_value(clamped_pts);
                        label.set_text(&format!(
                            "{} / {}",
                            format_time_display(clamped_pts),
                            format_time_display(total_dur)
                        ));
                        *is_updating.borrow_mut() = false;
                    }

                    // Playback reached end
                    if let Some(mut active) = active_playback_for_recv.borrow_mut().take() {
                        active.stop_flag.store(true, Ordering::SeqCst);
                        if let Some(mut child) = active.audio_child.take() {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                        if let Some(mut child) = active.video_child.take() {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                        btn.set_label("Play");
                        btn.set_icon_name("media-playback-start-symbolic");
                    }
                });

                *active_playback.borrow_mut() = Some(ActivePlayback {
                    audio_child,
                    video_child: Some(video_child),
                    stop_flag,
                });

                play_preview_btn_clone.set_label("Pause");
                play_preview_btn_clone.set_icon_name("media-playback-pause-symbolic");
            }
        });
    }

    // Set as Start button
    {
        let timeline_scale = timeline_scale.clone();
        let trim_start_entry = trim_start_entry.clone();
        set_start_btn.connect_clicked(move |_| {
            let val = timeline_scale.value();
            trim_start_entry.set_text(&format_time_display(val));
        });
    }

    // Set as End button
    {
        let timeline_scale = timeline_scale.clone();
        let trim_end_entry = trim_end_entry.clone();
        set_end_btn.connect_clicked(move |_| {
            let val = timeline_scale.value();
            trim_end_entry.set_text(&format_time_display(val));
        });
    }

    // Clear Trim marks button
    {
        let trim_start_entry = trim_start_entry.clone();
        let trim_end_entry = trim_end_entry.clone();
        clear_trim_btn.connect_clicked(move |_| {
            trim_start_entry.set_text("");
            trim_end_entry.set_text("");
        });
    }

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

    // Success Actions: Open Folder Button
    let open_folder_btn = Button::builder()
        .label("Open Output Folder")
        .css_classes(["flat"])
        .icon_name("folder-open-symbolic")
        .halign(gtk4::Align::Center)
        .visible(false)
        .build();

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

    let reset_btn = Button::with_label("Choose other files");
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
    actions_box.append(&open_folder_btn);
    actions_box.append(&reset_btn);
    actions_group.add(&actions_box);

    let details_container = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    details_container.append(&preview_picture);
    details_container.append(&file_group);
    details_container.append(&trim_group);
    details_container.append(&actions_group);

    let clamp = Clamp::builder()
        .maximum_size(680)
        .tightening_threshold(540)
        .child(&details_container)
        .build();

    let scrolled_details = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .child(&clamp)
        .build();

    view_stack.set_vexpand(true);
    view_stack.set_hexpand(true);
    view_stack.add_titled(&scrolled_details, Some("actions"), "Actions");
    view_stack.set_visible_child_name("drop");

    // Layout
    let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content_box.set_vexpand(true);
    content_box.append(&header_bar);
    content_box.append(&view_stack);
    toast_overlay.set_child(Some(&content_box));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("QuickCompress")
        .default_width(580)
        .default_height(680)
        .content(&toast_overlay)
        .build();

    // Clean up preview child processes on window close
    {
        let active_playback = active_playback.clone();
        window.connect_close_request(move |_| {
            if let Some(mut active) = active_playback.borrow_mut().take() {
                active.stop_flag.store(true, Ordering::SeqCst);
                if let Some(mut child) = active.audio_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                if let Some(mut child) = active.video_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            glib::Propagation::Proceed
        });
    }

    // Connect preferences button
    {
        let window_clone = window.clone();
        let app_config_clone = app_config.clone();
        prefs_btn.connect_clicked(move |_| {
            show_preferences_window(&window_clone, app_config_clone.clone());
        });
    }

    // Startup background check for updates
    {
        let window_clone = window.clone();
        let (sender, receiver) = async_channel::bounded::<updater::UpdateCheckResult>(1);

        thread::spawn(move || {
            if let Ok(info) = updater::check_for_updates() {
                if info.has_update {
                    let _ = sender.send_blocking(info);
                }
            }
        });

        glib::MainContext::default().spawn_local(async move {
            if let Ok(info) = receiver.recv().await {
                prompt_update_dialog(&window_clone, &info);
            }
        });
    }

    let current_files: Rc<RefCell<Vec<FileItem>>> = Rc::new(RefCell::new(Vec::new()));
    let last_output_dir: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));

    // Connect open folder button
    {
        let last_output_dir = last_output_dir.clone();
        open_folder_btn.connect_clicked(move |_| {
            if let Some(dir) = last_output_dir.borrow().as_ref() {
                open_folder_containing(dir);
            }
        });
    }

    // Helper: update UI on files loaded (single or batch)
    let update_files_selected = {
        let view_stack = view_stack.clone();
        let file_row = file_row.clone();
        let file_row_icon = file_row_icon.clone();
        let preview_picture = preview_picture.clone();
        let current_files = current_files.clone();
        let trim_group = trim_group.clone();
        let actions_group = actions_group.clone();
        let opt_discord_btn = opt_discord_btn.clone();
        let opt_email_btn = opt_email_btn.clone();
        let opt_webp_lossy_btn = opt_webp_lossy_btn.clone();
        let opt_webp_lossless_btn = opt_webp_lossless_btn.clone();
        let opt_jpeg_chat_btn = opt_jpeg_chat_btn.clone();
        let opt_resize_50_btn = opt_resize_50_btn.clone();
        let opt_strip_exif_btn = opt_strip_exif_btn.clone();
        let open_folder_btn = open_folder_btn.clone();
        let clear_header_btn = clear_header_btn.clone();
        let video_preview_picture = video_preview_picture.clone();
        let timeline_scale = timeline_scale.clone();
        let time_pos_label = time_pos_label.clone();
        let trim_start_entry = trim_start_entry.clone();
        let trim_end_entry = trim_end_entry.clone();
        let video_duration_state = video_duration_state.clone();
        let current_video_path = current_video_path.clone();

        move |paths: Vec<PathBuf>| {
            let mut items = Vec::new();
            let mut total_size: u64 = 0;
            let mut has_video = false;
            let mut has_image = false;

            for path in paths {
                if let Ok(metadata) = fs::metadata(&path) {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    let size = metadata.len();
                    let is_video = is_video_file(&path);

                    if is_video {
                        has_video = true;
                    } else {
                        has_image = true;
                    }
                    total_size += size;

                    items.push(FileItem {
                        path,
                        name,
                        size_bytes: size,
                        is_video,
                    });
                }
            }

            if items.is_empty() {
                return;
            }

            open_folder_btn.set_visible(false);

            if items.len() == 1 {
                let first = &items[0];
                file_row.set_title(&first.name);
                file_row.set_subtitle(&format!("Original size: {}", format_bytes(first.size_bytes)));
                file_row_icon.set_icon_name(Some(if first.is_video {
                    "video-x-generic-symbolic"
                } else {
                    "image-x-generic-symbolic"
                }));

                // Load thumbnail if it's an image, or setup video scrubber
                if !first.is_video {
                    *current_video_path.borrow_mut() = None;
                    video_preview_picture.set_visible(false);
                    preview_picture.set_filename(Some(&first.path));
                    preview_picture.set_visible(true);
                    trim_group.set_visible(false);
                } else {
                    *current_video_path.borrow_mut() = Some(first.path.clone());
                    preview_picture.set_visible(false);
                    trim_group.set_visible(true);
                    trim_start_entry.set_text("");
                    trim_end_entry.set_text("");

                    // Probe duration with ffprobe for accurate range
                    let probed_duration = video::probe_video_duration(&first.path).unwrap_or(0.0);
                    *video_duration_state.borrow_mut() = probed_duration;

                    timeline_scale.set_range(0.0, probed_duration.max(1.0));
                    timeline_scale.set_value(0.0);
                    time_pos_label.set_text(&format!("00:00 / {}", format_time_display(probed_duration)));

                    // Extract initial frame for instant visual preview
                    let vid_path = first.path.clone();
                    let (sender, receiver) = async_channel::bounded::<PathBuf>(1);
                    let pic = video_preview_picture.clone();

                    thread::spawn(move || {
                        let temp_frame = get_next_preview_frame_path();
                        if video::extract_video_frame(&vid_path, 0.0, &temp_frame).is_ok() {
                            let _ = sender.send_blocking(temp_frame);
                        }
                    });

                    glib::MainContext::default().spawn_local(async move {
                        if let Ok(frame_path) = receiver.recv().await {
                            pic.set_filename(Some(&frame_path));
                            pic.set_visible(true);
                        }
                    });
                }
            } else {
                *current_video_path.borrow_mut() = None;
                file_row.set_title(&format!("{} files selected", items.len()));
                file_row.set_subtitle(&format!("Total size: {}", format_bytes(total_size)));
                file_row_icon.set_icon_name(Some("emblem-documents-symbolic"));
                preview_picture.set_visible(false);
                video_preview_picture.set_visible(false);
                trim_group.set_visible(false);
            }

            // Adapt action buttons based on file types
            if has_video && !has_image {
                actions_group.set_title(if items.len() > 1 {
                    "Batch Video Compression"
                } else {
                    "Quick Video Compression"
                });
                opt_discord_btn.set_visible(true);
                opt_email_btn.set_visible(true);
                opt_webp_lossy_btn.set_visible(false);
                opt_webp_lossless_btn.set_visible(false);
                opt_jpeg_chat_btn.set_visible(false);
                opt_resize_50_btn.set_visible(false);
                opt_strip_exif_btn.set_visible(false);
            } else {
                actions_group.set_title(if items.len() > 1 {
                    "Batch Image Actions"
                } else {
                    "Quick Image Actions"
                });
                opt_discord_btn.set_visible(false);
                opt_email_btn.set_visible(false);
                opt_webp_lossy_btn.set_visible(true);
                opt_webp_lossless_btn.set_visible(true);
                opt_jpeg_chat_btn.set_visible(true);
                opt_resize_50_btn.set_visible(true);
                opt_strip_exif_btn.set_visible(true);
            }

            *current_files.borrow_mut() = items;
            clear_header_btn.set_visible(true);
            view_stack.set_visible_child_name("actions");
        }
    };

    // Reset button & Header Clear button
    {
        let view_stack = view_stack.clone();
        let current_files = current_files.clone();
        let preview_picture = preview_picture.clone();
        let video_preview_picture = video_preview_picture.clone();
        let active_playback = active_playback.clone();
        let play_preview_btn = play_preview_btn.clone();
        let trim_group = trim_group.clone();
        let open_folder_btn = open_folder_btn.clone();
        let clear_header_btn_clone = clear_header_btn.clone();

        let do_clear = move || {
            current_files.borrow_mut().clear();
            *current_video_path.borrow_mut() = None;
            if let Some(mut active) = active_playback.borrow_mut().take() {
                active.stop_flag.store(true, Ordering::SeqCst);
                if let Some(mut child) = active.audio_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                if let Some(mut child) = active.video_child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            play_preview_btn.set_label("Play");
            play_preview_btn.set_icon_name("media-playback-start-symbolic");
            preview_picture.set_visible(false);
            video_preview_picture.set_visible(false);
            trim_group.set_visible(false);
            open_folder_btn.set_visible(false);
            clear_header_btn_clone.set_visible(false);
            view_stack.set_visible_child_name("drop");
        };

        let do_clear_clone = do_clear.clone();
        reset_btn.connect_clicked(move |_| {
            do_clear_clone();
        });

        clear_header_btn.connect_clicked(move |_| {
            do_clear();
        });
    }

    // Helper: Paste from Clipboard
    let handle_paste_clipboard = {
        let update_files = update_files_selected.clone();
        let toast_overlay = toast_overlay.clone();

        move || {
            let display = gdk::Display::default().unwrap();
            let clipboard = display.clipboard();

            let update = update_files.clone();
            let toast = toast_overlay.clone();

            clipboard.read_texture_async(gio::Cancellable::NONE, move |result| {
                match result {
                    Ok(Some(texture)) => {
                        let temp_dir = env::temp_dir();
                        let temp_path = temp_dir.join(format!("clipboard_{}.png", glib::monotonic_time()));
                        if texture.save_to_png(&temp_path).is_ok() {
                            update(vec![temp_path]);
                            toast.add_toast(Toast::new("Pasted image from clipboard"));
                        } else {
                            toast.add_toast(Toast::new("Failed to save clipboard image"));
                        }
                    }
                    _ => {
                        toast.add_toast(Toast::new("No image found in clipboard"));
                    }
                }
            });
        }
    };

    // Paste button clicked
    {
        let handle_paste = handle_paste_clipboard.clone();
        paste_btn.connect_clicked(move |_| {
            handle_paste();
        });
    }

    // Key Controller: Global Ctrl+V to paste
    {
        let key_controller = EventControllerKey::new();
        let handle_paste = handle_paste_clipboard.clone();
        key_controller.connect_key_pressed(move |_, key, _, state| {
            if state.contains(gdk::ModifierType::CONTROL_MASK)
                && (key == gdk::Key::v || key == gdk::Key::V)
            {
                handle_paste();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key_controller);
    }

    // Drag and Drop Target (supports multiple files)
    let drop_target = gtk4::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    {
        let update_files = update_files_selected.clone();
        let toast_overlay = toast_overlay.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            if let Ok(file_list) = value.get::<gdk::FileList>() {
                let valid_paths: Vec<PathBuf> = file_list
                    .files()
                    .into_iter()
                    .filter_map(|f| f.path())
                    .filter(|p| is_supported_file(p))
                    .collect();

                if !valid_paths.is_empty() {
                    update_files(valid_paths);
                    return true;
                } else {
                    toast_overlay.add_toast(Toast::new("No supported files found in drop"));
                }
            }
            false
        });
    }
    window.add_controller(drop_target);

    // File Chooser Button Dialog (Multiple selection allowed)
    {
        let window_clone = window.clone();
        let update_files = update_files_selected.clone();
        open_file_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::new();
            dialog.set_title("Choose Files to Compress");

            let update = update_files.clone();
            dialog.open_multiple(Some(&window_clone), gio::Cancellable::NONE, move |result| {
                if let Ok(list_model) = result {
                    let mut paths = Vec::new();
                    for i in 0..list_model.n_items() {
                        if let Some(file) = list_model.item(i).and_then(|obj| obj.downcast::<gio::File>().ok()) {
                            if let Some(path) = file.path() {
                                if is_supported_file(&path) {
                                    paths.push(path);
                                }
                            }
                        }
                    }
                    if !paths.is_empty() {
                        update(paths);
                    }
                }
            });
        });
    }

    // Helper: Parse seconds from string like "12.5" or "01:23"
    let parse_time_str = |s: &str| -> Option<f64> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        if s.contains(':') {
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() == 2 {
                let mins: f64 = parts[0].parse().ok()?;
                let secs: f64 = parts[1].parse().ok()?;
                return Some(mins * 60.0 + secs);
            }
        }

        s.parse::<f64>().ok()
    };

    // Helper for async video compression (supports batch + trim)
    let trigger_video_compression = {
        let current_files = current_files.clone();
        let app_config = app_config.clone();
        let toast_overlay = toast_overlay.clone();
        let trim_start_entry = trim_start_entry.clone();
        let trim_end_entry = trim_end_entry.clone();
        let opt_discord_btn = opt_discord_btn.clone();
        let opt_email_btn = opt_email_btn.clone();
        let open_folder_btn = open_folder_btn.clone();
        let last_output_dir = last_output_dir.clone();
        let spinner = spinner.clone();
        let progress_bar = progress_bar.clone();
        let status_label = status_label.clone();
        let reset_btn = reset_btn.clone();

        move |target_mb: f64| {
            let items = current_files.borrow().clone();
            if items.is_empty() {
                return;
            }

            let start_sec = parse_time_str(&trim_start_entry.text());
            let end_sec = parse_time_str(&trim_end_entry.text());
            let custom_dir = app_config.borrow().custom_save_dir.clone();

            opt_discord_btn.set_sensitive(false);
            opt_email_btn.set_sensitive(false);
            reset_btn.set_sensitive(false);
            open_folder_btn.set_visible(false);
            spinner.set_visible(true);
            spinner.start();
            progress_bar.set_visible(true);
            status_label.set_visible(true);
            status_label.set_text(&format!(
                "Compressing {} video(s) to < {:.0} MB...",
                items.len(),
                target_mb
            ));

            let (sender, receiver) = async_channel::bounded::<Result<(PathBuf, usize, usize), String>>(1);

            let pbar = progress_bar.clone();
            let pulse_timer = glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                pbar.pulse();
                glib::ControlFlow::Continue
            });

            let items_to_process = items.clone();
            thread::spawn(move || {
                let total = items_to_process.len();
                let mut last_saved = PathBuf::new();

                for (idx, item) in items_to_process.iter().enumerate() {
                    let out_suffix = format!("{:.0}mb", target_mb);
                    let out_path = video::generate_output_path(
                        &item.path,
                        custom_dir.as_deref(),
                        &out_suffix,
                        "mp4",
                    );

                    if let Err(e) = video::compress_video_target_mb(
                        &item.path,
                        &out_path,
                        target_mb,
                        start_sec,
                        end_sec,
                    ) {
                        let _ = sender.send_blocking(Err(e));
                        return;
                    }
                    last_saved = out_path;
                    let _ = sender.send_blocking(Ok((last_saved.clone(), idx + 1, total)));
                }
            });

            let toast_overlay = toast_overlay.clone();
            let opt_discord = opt_discord_btn.clone();
            let opt_email = opt_email_btn.clone();
            let open_folder = open_folder_btn.clone();
            let last_dir = last_output_dir.clone();
            let reset = reset_btn.clone();
            let spin = spinner.clone();
            let pbar = progress_bar.clone();
            let status_lbl = status_label.clone();

            glib::MainContext::default().spawn_local(async move {
                while let Ok(msg) = receiver.recv().await {
                    match msg {
                        Ok((saved_path, current, total)) => {
                            status_lbl.set_text(&format!("Compressed {}/{}...", current, total));
                            if let Some(parent) = saved_path.parent() {
                                *last_dir.borrow_mut() = Some(parent.to_path_buf());
                            }

                            if current == total {
                                pulse_timer.remove();
                                spin.stop();
                                spin.set_visible(false);
                                pbar.set_visible(false);
                                status_lbl.set_visible(false);
                                opt_discord.set_sensitive(true);
                                opt_email.set_sensitive(true);
                                reset.set_sensitive(true);
                                open_folder.set_visible(true);

                                let notice = if total == 1 {
                                    let size_str = fs::metadata(&saved_path)
                                        .map(|m| format_bytes(m.len()))
                                        .unwrap_or_default();
                                    format!(
                                        "Saved: {} ({})",
                                        saved_path.file_name().and_then(|n| n.to_str()).unwrap_or("video.mp4"),
                                        size_str
                                    )
                                } else {
                                    format!("Successfully compressed all {} videos!", total)
                                };
                                toast_overlay.add_toast(Toast::new(&notice));
                                break;
                            }
                        }
                        Err(err) => {
                            pulse_timer.remove();
                            spin.stop();
                            spin.set_visible(false);
                            pbar.set_visible(false);
                            status_lbl.set_visible(false);
                            opt_discord.set_sensitive(true);
                            opt_email.set_sensitive(true);
                            reset.set_sensitive(true);
                            toast_overlay.add_toast(Toast::new(&format!("Error: {}", err)));
                            break;
                        }
                    }
                }
            });
        }
    };

    // Helper for async image compression (supports batch + custom save directory)
    let trigger_image_compression = {
        let current_files = current_files.clone();
        let app_config = app_config.clone();
        let toast_overlay = toast_overlay.clone();
        let opt_webp_lossy_btn = opt_webp_lossy_btn.clone();
        let opt_webp_lossless_btn = opt_webp_lossless_btn.clone();
        let opt_jpeg_chat_btn = opt_jpeg_chat_btn.clone();
        let opt_resize_50_btn = opt_resize_50_btn.clone();
        let opt_strip_exif_btn = opt_strip_exif_btn.clone();
        let open_folder_btn = open_folder_btn.clone();
        let last_output_dir = last_output_dir.clone();
        let spinner = spinner.clone();
        let progress_bar = progress_bar.clone();
        let status_label = status_label.clone();
        let reset_btn = reset_btn.clone();

        // mode: 0 = WebP Lossy, 1 = WebP Lossless, 2 = JPEG Chat, 3 = Scale 50%, 4 = Strip EXIF
        move |mode: u8| {
            let items = current_files.borrow().clone();
            if items.is_empty() {
                return;
            }

            let custom_dir = app_config.borrow().custom_save_dir.clone();

            opt_webp_lossy_btn.set_sensitive(false);
            opt_webp_lossless_btn.set_sensitive(false);
            opt_jpeg_chat_btn.set_sensitive(false);
            opt_resize_50_btn.set_sensitive(false);
            opt_strip_exif_btn.set_sensitive(false);
            reset_btn.set_sensitive(false);
            open_folder_btn.set_visible(false);
            spinner.set_visible(true);
            spinner.start();
            progress_bar.set_visible(true);
            status_label.set_visible(true);
            status_label.set_text(match mode {
                0 => "Optimizing to WebP (Lossy ~78%)...",
                1 => "Converting to WebP (Lossless)...",
                2 => "Compressing JPEG for chat...",
                3 => "Scaling images down 50%...",
                _ => "Stripping EXIF & GPS metadata...",
            });

            let (sender, receiver) = async_channel::bounded::<Result<(PathBuf, usize, usize, u64, u64), String>>(1);

            let pbar = progress_bar.clone();
            let pulse_timer = glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
                pbar.pulse();
                glib::ControlFlow::Continue
            });

            let items_to_process = items.clone();
            thread::spawn(move || {
                let total = items_to_process.len();
                for (idx, item) in items_to_process.iter().enumerate() {
                    let orig_ext = item
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
                    let out_path = image_ops::generate_image_output_path(
                        &item.path,
                        custom_dir.as_deref(),
                        out_suffix,
                        ext,
                    );

                    let res = match mode {
                        0 => image_ops::convert_to_webp(&item.path, &out_path, false),
                        1 => image_ops::convert_to_webp(&item.path, &out_path, true),
                        2 => image_ops::optimize_jpeg(&item.path, &out_path, 1920),
                        3 => image_ops::resize_image(&item.path, &out_path, Some(0.5), None),
                        _ => image_ops::strip_metadata(&item.path, &out_path),
                    };

                    match res {
                        Ok(_) => {
                            let new_len = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                            let _ = sender.send_blocking(Ok((out_path, idx + 1, total, item.size_bytes, new_len)));
                        }
                        Err(e) => {
                            let _ = sender.send_blocking(Err(e));
                            return;
                        }
                    }
                }
            });

            let toast_overlay = toast_overlay.clone();
            let opt_webp_lossy = opt_webp_lossy_btn.clone();
            let opt_webp_lossless = opt_webp_lossless_btn.clone();
            let opt_jpeg = opt_jpeg_chat_btn.clone();
            let opt_resize = opt_resize_50_btn.clone();
            let opt_strip = opt_strip_exif_btn.clone();
            let open_folder = open_folder_btn.clone();
            let last_dir = last_output_dir.clone();
            let reset = reset_btn.clone();
            let spin = spinner.clone();
            let pbar = progress_bar.clone();
            let status_lbl = status_label.clone();

            glib::MainContext::default().spawn_local(async move {
                while let Ok(msg) = receiver.recv().await {
                    match msg {
                        Ok((saved_path, current, total, orig_size, new_size)) => {
                            status_lbl.set_text(&format!("Processed {}/{}...", current, total));
                            if let Some(parent) = saved_path.parent() {
                                *last_dir.borrow_mut() = Some(parent.to_path_buf());
                            }

                            if current == total {
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
                                open_folder.set_visible(true);

                                let notice = if total == 1 {
                                    let saved_pct = if orig_size > new_size && orig_size > 0 {
                                        ((orig_size - new_size) as f64 / orig_size as f64) * 100.0
                                    } else {
                                        0.0
                                    };

                                    if saved_pct > 0.0 {
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
                                    }
                                } else {
                                    format!("Successfully processed all {} images!", total)
                                };
                                toast_overlay.add_toast(Toast::new(&notice));
                                break;
                            }
                        }
                        Err(err) => {
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
                            toast_overlay.add_toast(Toast::new(&format!("Error: {}", err)));
                            break;
                        }
                    }
                }
            });
        }
    };

    // Connect video compression buttons
    {
        let trigger = trigger_video_compression.clone();
        opt_discord_btn.connect_clicked(move |_| {
            trigger(9.5);
        });
    }

    {
        let trigger = trigger_video_compression;
        opt_email_btn.connect_clicked(move |_| {
            trigger(24.0);
        });
    }

    // Connect image compression buttons
    {
        let trigger = trigger_image_compression.clone();
        opt_webp_lossy_btn.connect_clicked(move |_| {
            trigger(0);
        });
    }

    {
        let trigger = trigger_image_compression.clone();
        opt_webp_lossless_btn.connect_clicked(move |_| {
            trigger(1);
        });
    }

    {
        let trigger = trigger_image_compression.clone();
        opt_jpeg_chat_btn.connect_clicked(move |_| {
            trigger(2);
        });
    }

    {
        let trigger = trigger_image_compression.clone();
        opt_resize_50_btn.connect_clicked(move |_| {
            trigger(3);
        });
    }

    {
        let trigger = trigger_image_compression;
        opt_strip_exif_btn.connect_clicked(move |_| {
            trigger(4);
        });
    }

    window.present();
}
