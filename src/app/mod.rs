use crate::{examples, runtime};
use eframe::egui;

pub struct ExplorerApp {
    examples: Vec<examples::Example>,
    status_message: String,
}

impl ExplorerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        log::info!("Initializing ExplorerApp");

        let examples = match examples::load_examples() {
            Ok(examples) => examples,
            Err(error) => {
                log::error!("Failed to load examples: {error}");
                Vec::new()
            }
        };

        Self {
            examples,
            status_message: String::from("Ready to explore Koto scripts"),
        }
    }
}

impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Koto Learning Explorer");
            ui.label(&self.status_message);

            if self.examples.is_empty() {
                ui.label("No examples available yet.");
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for example in &self.examples {
                        ui.group(|ui| {
                            ui.heading(&example.title);
                            ui.label(&example.description);
                            if ui.button("Run example").clicked() {
                                match runtime::RUNTIME.evaluate_script(&example.source) {
                                    Ok(result) => self.status_message = result,
                                    Err(error) => {
                                        self.status_message = format!("Error: {error}");
                                    }
                                }
                            }
                        });
                        ui.add_space(8.0);
                    }
                });
            }
        });
    }
}
