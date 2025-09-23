use crate::{examples, runtime};
use eframe::egui;

pub struct ExplorerApp {
    example_library: Option<&'static examples::ExampleLibrary>,
    examples: Vec<examples::Example>,
    examples_version: usize,
    status_message: String,
}

impl ExplorerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        log::info!("Initializing ExplorerApp");

        let (example_library, examples, examples_version) = match examples::library() {
            Ok(library) => {
                let snapshot = library.snapshot();
                (Some(library), snapshot, library.version())
            }
            Err(error) => {
                log::error!("Failed to initialize example library: {error}");
                (None, Vec::new(), 0)
            }
        };

        Self {
            example_library,
            examples,
            examples_version,
            status_message: String::from("Ready to explore Koto scripts"),
        }
    }
}

impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(library) = self.example_library {
            let version = library.version();
            if version != self.examples_version {
                self.examples = library.snapshot();
                self.examples_version = version;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Koto Learning Explorer");
            ui.label(&self.status_message);

            if self.examples.is_empty() {
                ui.label("No examples available yet.");
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for example in &self.examples {
                        ui.group(|ui| {
                            ui.heading(&example.metadata.title);
                            ui.label(&example.metadata.description);
                            if let Some(note) = &example.metadata.note {
                                ui.label(note);
                            }
                            if !example.metadata.categories.is_empty() {
                                let categories = example.metadata.categories.join(", ");
                                ui.label(format!("Tags: {categories}"));
                            }
                            if let Some(doc_url) = &example.metadata.doc_url {
                                ui.hyperlink(doc_url);
                            }
                            if let Some(run_instructions) = &example.metadata.run_instructions {
                                ui.label(run_instructions);
                            }
                            if ui.button("Run example").clicked() {
                                match runtime::RUNTIME.execute_script(&example.script) {
                                    Ok(result) => {
                                        let mut message = String::new();
                                        if let Some(value) = result.return_value {
                                            message.push_str(&format!("Result: {value}\n"));
                                        }
                                        if !result.stdout.is_empty() {
                                            message
                                                .push_str(&format!("Stdout:\n{}\n", result.stdout));
                                        }
                                        if !result.stderr.is_empty() {
                                            message
                                                .push_str(&format!("Stderr:\n{}", result.stderr));
                                        }
                                        if message.is_empty() {
                                            message.push_str("Example executed with no output");
                                        }
                                        self.status_message = message;
                                    }
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
