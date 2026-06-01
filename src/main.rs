mod app;
mod client;
mod storage;
mod theme;
mod environments;

fn main() -> Result<(), eframe::Error> {
    // Enable logging or other setup if needed
    
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AeroClient")
            .with_inner_size(egui::vec2(1200.0, 800.0))
            .with_min_inner_size(egui::vec2(800.0, 600.0)),
        ..Default::default()
    };

    eframe::run_native(
        "AeroClient API Client",
        native_options,
        Box::new(|cc| Box::new(app::AeroApp::new(cc))),
    )
}
