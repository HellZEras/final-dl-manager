use crate::{
    colors::{CYAN, GRAY, GREEN, PURPLE, RED},
    DownloadManager,
};
use egui_sfml::egui::{menu, Align, Color32, CursorIcon, Layout, RichText, TextEdit};
use std::fs::{read_dir, remove_file};

pub fn init_menu_bar(interface: &mut DownloadManager, ui: &mut egui_sfml::egui::Ui) {
    menu::bar(ui, |ui| {
        let text = RichText::new("Files").color(*CYAN).strong().size(15.0);
        ui.add_space(5.0);

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.menu_button(text, |ui| {
                    file_button_content(interface, ui);
                });
                let text = RichText::new("Downloads").color(*CYAN).strong().size(15.0);
                ui.menu_button(text, |ui| {
                    let text = RichText::new("Add Download").color(*CYAN).strong();
                    if ui.button(text).clicked() {
                        interface.popups.download.show = true;
                    }
                    let text = RichText::new("Resume all").color(*CYAN).strong();
                    if ui.button(text).clicked() {
                        for core in interface.files.iter_mut() {
                            core.file
                                .running
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    let text = RichText::new("Pause all").color(*CYAN).strong();
                    if ui.button(text).clicked() {
                        for core in interface.files.iter_mut() {
                            core.file
                                .running
                                .store(false, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    let text = RichText::new("Delete all completed").color(*CYAN).strong();
                    if ui.button(text).clicked() {
                        interface.files.retain(|core| {
                            !core
                                .file
                                .complete
                                .load(std::sync::atomic::Ordering::Relaxed)
                        });
                    }
                    let text = RichText::new("Delete all incomplete").color(*CYAN).strong();
                    if ui.button(text).clicked() {
                        interface.files.retain(|core| {
                            core.file
                                .complete
                                .load(std::sync::atomic::Ordering::Relaxed)
                        });
                    }
                });
            });
        });
        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
            ui.add_space(5.0);

            let text = egui_sfml::egui::RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS)
                .size(19.0)
                .color(*CYAN);
            if ui.label(text).hovered() {
                ui.output_mut(|o| o.cursor_icon = CursorIcon::Default)
            }

            ui.scope(|ui| {
                ui.visuals_mut().extreme_bg_color = *CYAN;
                ui.set_width(250.0);
                ui.visuals_mut().override_text_color = Some(*PURPLE);
                let hint_text = RichText::new("Filename or Url").color(*GRAY);
                let single_line = TextEdit::singleline(&mut interface.search).hint_text(hint_text);
                ui.add(single_line);
            });
        });
    });
}

fn file_button_content(interface: &mut DownloadManager, ui: &mut egui_sfml::egui::Ui) {
    let text = RichText::new("Remove selected from list")
        .color(*CYAN)
        .strong();
    if ui.button(text).clicked() {
        interface.popups.confirm.color = Color32::GREEN;
        interface.popups.confirm.task = Box::new(|| {
            Box::new(move |app: &mut DownloadManager| {
                app.files.retain(|core| !core.selected);
            })
        });
        interface.popups.confirm.show = true;
        interface.popups.confirm.text = String::from("This will remove files selected from list")
    }
    let text = RichText::new("Remove selected from disk")
        .color(*CYAN)
        .strong();
    if ui.button(text).clicked() {
        interface.popups.confirm.color = *RED;
        interface.popups.confirm.task = Box::new(|| {
            Box::new(move |app: &mut DownloadManager| {
                remove_selected_from_disk(app);
            })
        });
        interface.popups.confirm.show = true;
        interface.popups.confirm.text = String::from("This will remove selected files from disk")
    }
    let text = RichText::new("Remove all from list").color(*CYAN).strong();
    if ui.button(text).clicked() {
        interface.popups.confirm.color = *GREEN;
        interface.popups.confirm.task = Box::new(|| {
            Box::new(move |app: &mut DownloadManager| {
                app.files.clear();
            })
        });
        interface.popups.confirm.show = true;
        interface.popups.confirm.text = "This will not delete files from disk".to_string();
    }
    let text = RichText::new("Remove all from disk").color(*CYAN).strong();
    if ui.button(text).clicked() {
        interface.popups.confirm.color = *RED;
        interface.popups.confirm.task = Box::new(|| {
            Box::new(move |app: &mut DownloadManager| {
                delete_all_files_from_disk(app);
            })
        });
        interface.popups.confirm.show = true;
        interface.popups.confirm.text = String::from("This will remove all files from disk")
    }
}
fn delete_all_files_from_disk(interface: &mut DownloadManager) {
    let dir = match read_dir("Downloads") {
        Ok(dir) => dir,
        Err(e) => {
            interface.popups.error.value = e.to_string();
            interface.popups.error.show = true;
            return;
        }
    };
    for file in dir {
        let path = match file {
            Ok(file) => file.path(),
            Err(e) => {
                interface.popups.error.value = e.to_string();
                interface.popups.error.show = true;
                return;
            }
        };
        remove_file(path).unwrap();
    }
    interface.files.clear();
}
fn remove_selected_from_disk(app: &mut DownloadManager) {
    app.files.retain(|core| {
        if core.selected {
            let path = format!("Downloads/{}", core.file.name_on_disk);
            let tmp_path = format!("Downloads/.{}.metadata", core.file.name_on_disk);
            match remove_file(path) {
                Ok(_) => {}
                Err(e) => {
                    app.popups.error.value = e.to_string();
                    app.popups.error.show = true;
                }
            }
            match remove_file(tmp_path) {
                Ok(_) => {}
                Err(e) => {
                    app.popups.error.value = e.to_string();
                    app.popups.error.show = true;
                }
            }
            return false;
        }
        true
    });
}
