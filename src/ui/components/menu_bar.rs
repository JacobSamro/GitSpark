use eframe::egui::{self, Color32, RichText, Stroke};

use crate::ui::theme::SURFACE_BG_MUTED;
use crate::ui::ui_state::SidebarTab;

pub enum MenuAction {
    OpenRepoDialog,
    ShowSettings,
    SetSidebarTab(SidebarTab),
    Push,
    Pull,
    Fetch,
    MergeBranch,
    Exit,
}

pub fn render_menu_bar(ctx: &egui::Context) -> Option<MenuAction> {
    let mut action = None;

    egui::TopBottomPanel::top("menu_bar")
        .exact_height(28.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::default()
                .fill(SURFACE_BG_MUTED)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .stroke(Stroke::new(0.0, Color32::TRANSPARENT)),
        )
        .show(ctx, |ui| {
            let previous_override = ui.visuals().override_text_color;
            ui.visuals_mut().override_text_color = Some(Color32::from_rgb(235, 240, 246));
            egui::menu::bar(ui, |ui| {
                ui.menu_button(RichText::new("File").color(Color32::WHITE), |ui| {
                    if ui.button("New Repository...").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Add Local Repository...").clicked() {
                        action = Some(MenuAction::OpenRepoDialog);
                        ui.close_menu();
                    }
                    if ui.button("Clone Repository...").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Options...").clicked() {
                        action = Some(MenuAction::ShowSettings);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        action = Some(MenuAction::Exit);
                    }
                });

                ui.menu_button(RichText::new("Edit").color(Color32::WHITE), |ui| {
                    let _ = ui.button("Undo");
                    let _ = ui.button("Redo");
                    ui.separator();
                    let _ = ui.button("Cut");
                    let _ = ui.button("Copy");
                    let _ = ui.button("Paste");
                    let _ = ui.button("Select All");
                });

                ui.menu_button(RichText::new("View").color(Color32::WHITE), |ui| {
                    if ui.button("Changes").clicked() {
                        action = Some(MenuAction::SetSidebarTab(SidebarTab::Changes));
                        ui.close_menu();
                    }
                    if ui.button("History").clicked() {
                        action = Some(MenuAction::SetSidebarTab(SidebarTab::History));
                        ui.close_menu();
                    }
                    ui.separator();
                    let _ = ui.button("Repository List");
                    ui.separator();
                    let _ = ui.button("Toggle Full Screen");
                });

                ui.menu_button(RichText::new("Repository").color(Color32::WHITE), |ui| {
                    if ui.button("Push").clicked() {
                        action = Some(MenuAction::Push);
                        ui.close_menu();
                    }
                    if ui.button("Pull").clicked() {
                        action = Some(MenuAction::Pull);
                        ui.close_menu();
                    }
                    if ui.button("Fetch").clicked() {
                        action = Some(MenuAction::Fetch);
                        ui.close_menu();
                    }
                    if ui.button("Remove...").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("View on GitHub").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Open in Terminal").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Show in Finder").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Repository Settings...").clicked() {
                        ui.close_menu();
                    }
                });

                ui.menu_button(RichText::new("Branch").color(Color32::WHITE), |ui| {
                    let _ = ui.button("New Branch...");
                    let _ = ui.button("Rename Branch...");
                    let _ = ui.button("Delete Branch...");
                    ui.separator();
                    let _ = ui.button("Update from Default Branch");
                    let _ = ui.button("Compare to Branch");
                    if ui.button("Merge into Current Branch...").clicked() {
                        action = Some(MenuAction::MergeBranch);
                        ui.close_menu();
                    }
                });

                ui.menu_button(RichText::new("Help").color(Color32::WHITE), |ui| {
                    let _ = ui.button("Report Issue...");
                    let _ = ui.button("Contact Support...");
                    ui.separator();
                    let _ = ui.button("Show Logs...");
                    ui.separator();
                    let _ = ui.button("About RustTop");
                });
            });
            ui.visuals_mut().override_text_color = previous_override;
        });

    action
}
