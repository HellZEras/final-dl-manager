use colors::{DARKER_PURPLE, PURPLE};
use dl::file2dl::File2Dl;
use download_mechanism::{check_urls, run_downloads, set_total_bandwidth, Actions};
use egui_aesthetix::{themes::TokyoNight, Aesthetix};
use egui_sfml::{
    egui::{Color32, Context, FontData, FontDefinitions, Id},
    sfml::{
        graphics::{FloatRect, RenderTarget, RenderWindow, View},
        window::{ContextSettings, Event, Style},
    },
    SfEgui,
};
use extern_windows::Bandwidth;
use menu_bar::init_menu_bar;
use popups::*;
use server::interception::init_server;
use status_bar::{check_connection, init_status_bar, Connection};
use std::{fs::remove_file, path::Path, sync::mpsc::channel, time::Duration};
use table::lay_table;
use tokio::runtime::{self, Runtime};
use tray::{handle_tray_events, Message, Tray};

mod colors;
mod dl;
mod download_mechanism;
mod extern_windows;
mod menu_bar;
mod popups;
mod server;
mod status_bar;
mod table;
mod tray;

struct DownloadManager {
    runtime: Runtime,
    files: Vec<FDl>,
    popups: PopUps,
    temp_action: Actions,
    search: String,
    connection: Connection,
    bandwidth: Bandwidth,
    tray_menu: Tray,
    show_window: bool,
}

impl DownloadManager {
    fn update(&mut self, ctx: &egui_sfml::egui::Context) {
        if !self.show_window {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        set_total_bandwidth(self);
        check_connection(self);
        run_downloads(self);
        check_urls(self);
        handle_popups(self, ctx);
        handle_tray_events(self);

        egui_sfml::egui::TopBottomPanel::top(Id::new("Top"))
            .exact_height(40.0)
            .frame(egui_sfml::egui::Frame::none().fill(*DARKER_PURPLE))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(7.0);
                });
                init_menu_bar(self, ui);
            });
        egui_sfml::egui::CentralPanel::default()
            .frame(
                egui_sfml::egui::Frame::none()
                    .fill(*PURPLE)
                    .inner_margin(TokyoNight.margin_style())
                    .stroke(egui_sfml::egui::Stroke::new(
                        1.0,
                        Color32::from_rgba_premultiplied(31, 31, 51, 255),
                    )),
            )
            .show(ctx, |ui| {
                lay_table(self, ui, ctx);
            });
        egui_sfml::egui::TopBottomPanel::bottom(Id::new("Bottom"))
            .exact_height(40.0)
            .show_separator_line(false)
            .frame(egui_sfml::egui::Frame::none().fill(*DARKER_PURPLE))
            .show(ctx, |ui| {
                init_status_bar(self, ui);
            });
    }

    fn default() -> Self {
        let runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build runtime");

        let popups = PopUps {
            error: Self::create_error_popup(),
            download: DownloadPopUp::default(),
            confirm: ConfirmPopUp::default(),
            plot: PLotPopUp::default(),
            speed: EditSpeedPopUp::default(),
        };

        let files = Self::load_files().unwrap_or_default();

        Self {
            runtime,
            files,
            popups,
            temp_action: Actions::default(),
            search: String::default(),
            connection: Connection::default(),
            bandwidth: Bandwidth::default(),
            tray_menu: Tray::default(),
            show_window: true,
        }
    }
    fn create_error_popup() -> ErrorPopUp {
        match File2Dl::from("Downloads") {
            Ok(_) => ErrorPopUp::default(),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => ErrorPopUp {
                value: e.to_string(),
                show: true,
                channel: channel(),
            },
            _ => ErrorPopUp::default(),
        }
    }

    fn load_files() -> Result<Vec<FDl>, std::io::Error> {
        let files = File2Dl::from("Downloads")?;
        Ok(files
            .into_iter()
            .map(|file| FDl {
                file,
                new: false,
                initiated: false,
                selected: false,
                action_on_save: Actions::default(),
            })
            .collect())
    }
}

#[derive(Debug, Default, Clone)]
struct FDl {
    file: File2Dl,
    new: bool,
    initiated: bool,
    selected: bool,
    action_on_save: Actions,
}

fn main() {
    let mut init_size = (860, 480);
    let title = "Rusty Dl Manager";
    let settings = &ContextSettings {
        depth_bits: 0,
        stencil_bits: 0,
        antialiasing_level: 0,
        ..Default::default()
    };

    let mut rw = RenderWindow::new(init_size, title, Style::DEFAULT, settings).unwrap();
    rw.set_framerate_limit(60);

    let mut sf_egui = SfEgui::new(&rw);
    setup_custom_fonts(sf_egui.context());

    let mut state = DownloadManager::default();

    state.runtime.spawn_blocking(move || {
        init_server().unwrap_or_default();
    });

    while rw.is_open() {
        while let Some(ev) = rw.poll_event() {
            sf_egui.add_event(&ev);
            if matches!(ev, Event::Closed) {
                state.popups.download.show = false;
                state.popups.confirm.show = false;
                state.popups.error.show = false;
                state.popups.plot.show = false;
                state.popups.speed.show = false;
                state.tray_menu.message = Message::None;
                state.show_window = false;
            }
            if let Event::Resized { width, height } = ev {
                init_size = (width, height);
                rw.set_view(
                    &View::from_rect(FloatRect::new(0f32, 0f32, width as f32, height as f32))
                        .unwrap(),
                );
            }
        }

        if state.show_window {
            rw.set_visible(true)
        } else {
            rw.set_visible(false);
        }
        if state.show_window && state.tray_menu.message == Message::Show {
            state.tray_menu.message = Message::None;
            rw.recreate(init_size, title, Style::DEFAULT, settings);
        }
        let di = sf_egui
            .run(&mut rw, |_rw, ctx| {
                state.update(ctx);
            })
            .unwrap();
        if state.show_window {
            sf_egui.draw(di, &mut rw, None);
            rw.display();
        }
    }
}

fn setup_custom_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts.font_data.insert(
        "my_font".to_owned(),
        FontData::from_static(include_bytes!("../JetBrainsMono-Regular.ttf")),
    );

    fonts
        .families
        .entry(egui_sfml::egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "my_font".to_owned());

    fonts
        .families
        .entry(egui_sfml::egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "my_font".to_owned());
    ctx.set_fonts(fonts);
}
