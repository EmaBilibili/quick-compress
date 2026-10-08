use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow, HeaderBar, StatusPage};

const APP_ID: &str = "io.github.EmaBilibili.QuickCompress";

fn main() {
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let header_bar = HeaderBar::new();

    let status_page = StatusPage::builder()
        .icon_name("document-send-symbolic")
        .title("QuickCompress")
        .description("Drag and drop images, video or audio files here to compress")
        .build();

    let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content_box.append(&header_bar);
    content_box.append(&status_page);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("QuickCompress")
        .default_width(480)
        .default_height(400)
        .content(&content_box)
        .build();

    window.present();
}
