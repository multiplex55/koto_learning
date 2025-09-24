use crate::{
    examples::{self, Example},
    runtime,
};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, RichText};
use egui_extras::syntax_highlighting;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::{Duration, Instant},
};

const LOG_POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_CONSOLE_ENTRIES: usize = 400;

pub struct ExplorerApp {
    example_library: Option<&'static examples::ExampleLibrary>,
    examples: Vec<Example>,
    examples_version: usize,
    selected_example_id: Option<String>,
    search_query: String,
    category_filters: BTreeSet<String>,
    console_entries: Vec<ConsoleEntry>,
    last_execution: Option<ExecutionSummary>,
    input_values: HashMap<String, String>,
    watch_mode_enabled: bool,
    hot_reload_enabled: bool,
    has_loaded_examples_once: bool,
    pending_hot_reload_run: bool,
    runtime_log_path: PathBuf,
    runtime_log_size: u64,
    last_log_poll: Option<Instant>,
    snackbars: Vec<Snackbar>,
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

        let selected_example_id = examples.first().map(|example| example.metadata.id.clone());
        let mut app = Self {
            example_library,
            examples,
            examples_version,
            selected_example_id,
            search_query: String::new(),
            category_filters: BTreeSet::new(),
            console_entries: vec![ConsoleEntry::info("Ready to explore Koto scripts")],
            last_execution: None,
            input_values: HashMap::new(),
            watch_mode_enabled: true,
            hot_reload_enabled: false,
            has_loaded_examples_once: false,
            pending_hot_reload_run: false,
            runtime_log_path: PathBuf::from("logs").join("runtime.log"),
            runtime_log_size: 0,
            last_log_poll: None,
            snackbars: Vec::new(),
        };

        if let Some(metadata) = app.examples.first().map(|example| example.metadata.clone()) {
            app.apply_input_defaults(&metadata);
        }
        if !app.examples.is_empty() {
            app.has_loaded_examples_once = true;
        }

        app
    }

    fn selected_example(&self) -> Option<&Example> {
        self.selected_example_id.as_ref().and_then(|id| {
            self.examples
                .iter()
                .find(|example| &example.metadata.id == id)
        })
    }

    fn ensure_examples_current(&mut self) {
        if !self.watch_mode_enabled {
            return;
        }

        if let Some(library) = self.example_library {
            let version = library.version();
            if version != self.examples_version {
                self.examples = library.snapshot();
                self.examples_version = version;
                self.on_examples_changed(true);
            }
        }
    }

    fn refresh_examples_from_library(&mut self) {
        if let Some(library) = self.example_library {
            self.examples = library.snapshot();
            self.examples_version = library.version();
            self.on_examples_changed(false);
            self.push_snackbar("Example catalog refreshed", SnackbarKind::Info);
        }
    }

    fn on_examples_changed(&mut self, triggered_by_watch: bool) {
        let previous_selection = self.selected_example_id.clone();

        if let Some(selected_id) = &self.selected_example_id {
            if !self
                .examples
                .iter()
                .any(|example| &example.metadata.id == selected_id)
            {
                self.selected_example_id = None;
            }
        }

        if self.selected_example_id.is_none() {
            self.selected_example_id = self
                .examples
                .first()
                .map(|example| example.metadata.id.clone());
        }

        if let Some(metadata) = self
            .selected_example_id
            .as_ref()
            .and_then(|id| {
                self.examples
                    .iter()
                    .find(|example| &example.metadata.id == id)
            })
            .map(|example| example.metadata.clone())
        {
            self.apply_input_defaults(&metadata);
        }

        if triggered_by_watch && self.has_loaded_examples_once && self.hot_reload_enabled {
            if let Some(previous) = previous_selection {
                if self
                    .selected_example_id
                    .as_ref()
                    .map(|current| current == &previous)
                    .unwrap_or(false)
                {
                    self.pending_hot_reload_run = true;
                }
            }
        }

        self.has_loaded_examples_once = true;
    }

    fn apply_input_defaults(&mut self, metadata: &examples::ExampleMetadata) {
        self.input_values.clear();
        for input in &metadata.inputs {
            let value = input.default.clone().unwrap_or_default();
            self.input_values.insert(input.name.clone(), value);
        }
    }

    fn select_example(&mut self, example_id: &str) {
        if self.selected_example_id.as_deref() == Some(example_id) {
            return;
        }

        self.selected_example_id = Some(example_id.to_string());
        if let Some(metadata) = self
            .examples
            .iter()
            .find(|example| example.metadata.id == example_id)
            .map(|example| example.metadata.clone())
        {
            self.apply_input_defaults(&metadata);
        }
        self.push_snackbar("Example selected", SnackbarKind::Info);
    }

    fn run_selected_example(&mut self) {
        let example = match self.selected_example().cloned() {
            Some(example) => example,
            None => {
                self.push_console_entry(ConsoleEntry::error("No example selected"));
                self.push_snackbar("Select an example before running", SnackbarKind::Error);
                return;
            }
        };

        let script = self.prepare_script(&example);
        self.push_console_entry(ConsoleEntry::info(format!(
            "Running '{}'",
            example.metadata.title
        )));

        match runtime::RUNTIME.execute_script(&script) {
            Ok(output) => {
                if let Some(value) = &output.return_value {
                    self.push_console_entry(ConsoleEntry::result(format!("Return value: {value}")));
                }
                if !output.stdout.is_empty() {
                    self.push_console_entry(ConsoleEntry::stdout(output.stdout.clone()));
                }
                if !output.stderr.is_empty() {
                    self.push_console_entry(ConsoleEntry::stderr(output.stderr.clone()));
                }
                if output.stdout.is_empty()
                    && output.stderr.is_empty()
                    && output.return_value.is_none()
                {
                    self.push_console_entry(ConsoleEntry::info("Example executed with no output"));
                }

                self.last_execution = Some(ExecutionSummary {
                    duration: output.duration,
                    return_value: output.return_value,
                    succeeded: true,
                });
                self.push_snackbar("Example executed successfully", SnackbarKind::Success);
            }
            Err(error) => {
                self.push_console_entry(ConsoleEntry::error(format!("Execution error: {error}")));
                self.last_execution = Some(ExecutionSummary {
                    duration: Duration::default(),
                    return_value: None,
                    succeeded: false,
                });
                self.push_snackbar("Example execution failed", SnackbarKind::Error);
            }
        }
    }

    fn prepare_script(&self, example: &Example) -> String {
        if self.input_values.is_empty() {
            return example.script.clone();
        }

        let json = serde_json::to_string(&self.input_values).unwrap_or_default();
        let escaped_json = json.replace('\\', "\\\\").replace('"', "\\\"");
        let mut prefix = String::from("import serde\n");
        prefix.push_str(&format!("input = serde.from_json(\"{}\")\n", escaped_json));
        format!("{prefix}{}", example.script)
    }

    fn push_console_entry(&mut self, entry: ConsoleEntry) {
        self.console_entries.push(entry);
        self.trim_console_history();
    }

    fn trim_console_history(&mut self) {
        if self.console_entries.len() > MAX_CONSOLE_ENTRIES {
            let excess = self.console_entries.len() - MAX_CONSOLE_ENTRIES;
            self.console_entries.drain(0..excess);
        }
    }

    fn push_snackbar(&mut self, message: impl Into<String>, kind: SnackbarKind) {
        self.snackbars.push(Snackbar {
            message: message.into(),
            kind,
            created: Instant::now(),
            duration: Duration::from_secs(4),
        });
    }

    fn poll_runtime_logs(&mut self) {
        let now = Instant::now();
        if self
            .last_log_poll
            .map(|previous| now.duration_since(previous) < LOG_POLL_INTERVAL)
            .unwrap_or(false)
        {
            return;
        }
        self.last_log_poll = Some(now);

        let path = &self.runtime_log_path;
        if !path.exists() {
            return;
        }

        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => return,
        };

        let len = metadata.len();
        if len < self.runtime_log_size {
            self.runtime_log_size = 0;
        }

        if len == self.runtime_log_size {
            return;
        }

        if let Ok(mut file) = File::open(path) {
            if file.seek(SeekFrom::Start(self.runtime_log_size)).is_ok() {
                let mut new_content = String::new();
                if file.read_to_string(&mut new_content).is_ok() {
                    for line in new_content.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }
                        self.push_console_entry(ConsoleEntry::log(line.trim().to_string()));
                    }
                }
            }
        }

        self.runtime_log_size = len;
    }

    fn grouped_examples(&self) -> Vec<(String, Vec<ExampleListEntry>)> {
        let mut groups: BTreeMap<String, Vec<ExampleListEntry>> = BTreeMap::new();
        for example in &self.examples {
            if !self.passes_filters(example) {
                continue;
            }

            if example.metadata.categories.is_empty() {
                groups
                    .entry("Uncategorized".to_string())
                    .or_default()
                    .push(ExampleListEntry {
                        id: example.metadata.id.clone(),
                        title: example.metadata.title.clone(),
                        note: example.metadata.note.clone(),
                    });
            } else {
                for category in &example.metadata.categories {
                    groups
                        .entry(category.clone())
                        .or_default()
                        .push(ExampleListEntry {
                            id: example.metadata.id.clone(),
                            title: example.metadata.title.clone(),
                            note: example.metadata.note.clone(),
                        });
                }
            }
        }
        groups.into_iter().collect()
    }

    fn passes_filters(&self, example: &Example) -> bool {
        if !self.category_filters.is_empty()
            && !example
                .metadata
                .categories
                .iter()
                .any(|category| self.category_filters.contains(category))
        {
            return false;
        }

        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }

        let matches_query = example.metadata.title.to_lowercase().contains(&query)
            || example.metadata.description.to_lowercase().contains(&query)
            || example
                .metadata
                .note
                .as_ref()
                .map(|note| note.to_lowercase().contains(&query))
                .unwrap_or(false)
            || example
                .metadata
                .categories
                .iter()
                .any(|category| category.to_lowercase().contains(&query))
            || example.metadata.id.to_lowercase().contains(&query);

        matches_query
    }

    fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Examples");
        ui.add_space(8.0);

        let search_response =
            ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Search examples"));
        if search_response.changed() {
            ui.ctx().request_repaint();
        }

        if !self.category_filters.is_empty() {
            let filters = self
                .category_filters
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            ui.colored_label(
                egui::Color32::from_rgb(120, 180, 240),
                format!("Filters: {filters}"),
            );
            if ui.button("Clear filters").clicked() {
                self.category_filters.clear();
            }
        }

        ui.add_space(8.0);

        let mut all_categories: BTreeSet<String> = BTreeSet::new();
        for example in &self.examples {
            for category in &example.metadata.categories {
                all_categories.insert(category.clone());
            }
        }

        if !all_categories.is_empty() {
            ui.label("Filter by category:");
            for category in all_categories {
                let mut is_selected = self.category_filters.contains(&category);
                if ui.checkbox(&mut is_selected, category.as_str()).changed() {
                    if is_selected {
                        self.category_filters.insert(category.clone());
                    } else {
                        self.category_filters.remove(&category);
                    }
                }
            }
            ui.separator();
        }

        if ui.button("Refresh catalog").clicked() {
            self.refresh_examples_from_library();
        }

        if self.examples.is_empty() {
            ui.label("No examples available yet.");
            return;
        }

        ui.add_space(8.0);
        let grouped_examples = self.grouped_examples();
        egui::ScrollArea::vertical()
            .id_salt("example_list")
            .show(ui, |ui| {
                for (category, entries) in grouped_examples {
                    egui::CollapsingHeader::new(category)
                        .default_open(true)
                        .show(ui, |ui| {
                            for entry in entries {
                                let selected = self
                                    .selected_example_id
                                    .as_ref()
                                    .map(|id| id == &entry.id)
                                    .unwrap_or(false);
                                let mut response =
                                    ui.selectable_label(selected, entry.title.as_str());
                                if let Some(note) = &entry.note {
                                    response = response.on_hover_text(note);
                                }
                                if response.clicked() {
                                    self.select_example(&entry.id);
                                }
                            }
                        });
                }
            });
    }

    fn main_panel_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(example) = self.selected_example().cloned() {
            ui.heading(&example.metadata.title);
            ui.label(&example.metadata.description);

            if let Some(note) = &example.metadata.note {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(180, 140, 50), note);
            }

            if !example.metadata.categories.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Categories:");
                    for category in &example.metadata.categories {
                        ui.label(RichText::new(category).italics());
                    }
                });
            }

            if let Some(instructions) = &example.metadata.run_instructions {
                ui.add_space(6.0);
                ui.label(RichText::new(instructions).strong());
            }

            if let Some(doc_url) = &example.metadata.doc_url {
                ui.add_space(6.0);
                ui.hyperlink(doc_url);
            }

            for link in &example.metadata.documentation {
                ui.hyperlink_to(&link.label, &link.url);
            }

            if !example.metadata.how_it_works.is_empty() {
                ui.add_space(10.0);
                egui::CollapsingHeader::new("How it works")
                    .default_open(true)
                    .show(ui, |ui| {
                        for paragraph in &example.metadata.how_it_works {
                            ui.label(paragraph);
                            ui.add_space(4.0);
                        }
                    });
            }

            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label("Code");
                let theme = syntax_highlighting::CodeTheme::from_memory(ctx, ui.style());
                egui::ScrollArea::both()
                    .id_salt("code_view")
                    .show(ui, |ui| {
                        syntax_highlighting::code_view_ui(ui, &theme, &example.script, "koto");
                    });
                theme.store_in_memory(ctx);
            });

            ui.add_space(10.0);
            if !example.metadata.inputs.is_empty() {
                ui.group(|ui| {
                    ui.heading("Inputs");
                    for input in &example.metadata.inputs {
                        let value = self
                            .input_values
                            .entry(input.name.clone())
                            .or_insert_with(|| input.default.clone().unwrap_or_default());
                        ui.horizontal(|ui| {
                            let label = input.label.as_deref().unwrap_or(input.name.as_str());
                            ui.label(label);
                            let mut text_edit = egui::TextEdit::singleline(value);
                            if let Some(placeholder) = &input.placeholder {
                                text_edit = text_edit.hint_text(placeholder);
                            }
                            ui.add(text_edit);
                        });
                        if let Some(description) = &input.description {
                            ui.label(RichText::new(description).small());
                        }
                    }
                });
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Run example").clicked() {
                    self.run_selected_example();
                }
                if ui.button("Clear output").clicked() {
                    self.console_entries.clear();
                }
                ui.toggle_value(&mut self.watch_mode_enabled, "Watch examples");
                ui.toggle_value(&mut self.hot_reload_enabled, "Hot reload");
            });

            if let Some(bench) = &example.metadata.benchmarks {
                ui.add_space(6.0);
                self.resource_row(ui, "⏱ Benchmarks", bench);
            }
            if let Some(tests) = &example.metadata.tests {
                self.resource_row(ui, "🧪 Tests", tests);
            }

            if let Some(summary) = &self.last_execution {
                ui.add_space(8.0);
                let status = if summary.succeeded {
                    RichText::new("Last execution succeeded")
                        .color(Color32::from_rgb(120, 200, 120))
                } else {
                    RichText::new("Last execution failed").color(Color32::from_rgb(220, 80, 80))
                };
                ui.label(status);
                ui.label(format!("Duration: {} ms", summary.duration.as_millis()));
                if let Some(return_value) = &summary.return_value {
                    ui.label(format!("Return value: {return_value}"));
                }
            }
        } else {
            ui.label("Select an example from the sidebar to get started.");
        }
    }

    fn resource_row(&self, ui: &mut egui::Ui, label: &str, resource: &examples::ExampleResource) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).strong());
            if let Some(description) = &resource.description {
                ui.label(description);
            }
            if let Some(url) = &resource.url {
                let link_label = resource.label.as_deref().unwrap_or("View details");
                ui.hyperlink_to(link_label, url);
            }
        });
    }

    fn console_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Console").strong());
            if ui.button("Copy").clicked() {
                let text = self
                    .console_entries
                    .iter()
                    .map(|entry| format!("{}", entry.message))
                    .collect::<Vec<_>>()
                    .join("\n");
                ctx.copy_text(text);
            }
            if ui.button("Clear").clicked() {
                self.console_entries.clear();
            }
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .id_salt("console_scroll")
            .show(ui, |ui| {
                for entry in &self.console_entries {
                    let visuals = ui.visuals();
                    let color = entry.kind.color(visuals);
                    let message = RichText::new(&entry.message).color(color);
                    ui.label(message);
                }
            });
    }

    fn show_snackbars(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.snackbars
            .retain(|snackbar| now.duration_since(snackbar.created) < snackbar.duration);

        for (index, snackbar) in self.snackbars.iter().enumerate() {
            let progress = now.duration_since(snackbar.created).as_secs_f32()
                / snackbar.duration.as_secs_f32();
            let offset_y = -20.0 - (index as f32 * 40.0);
            egui::Area::new(egui::Id::new(format!("snackbar_{index}")))
                .anchor(Align2::CENTER_BOTTOM, [0.0, offset_y])
                .interactable(false)
                .show(ctx, |ui| {
                    let tint = snackbar.kind.color(ui.visuals());
                    let background = tint.gamma_multiply(0.2);
                    let frame = egui::Frame::new()
                        .fill(background)
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::same(8));
                    frame.show(ui, |ui| {
                        ui.colored_label(tint, &snackbar.message);
                        ui.add(
                            egui::ProgressBar::new(1.0 - progress.clamp(0.0, 1.0))
                                .desired_width(120.0),
                        );
                    });
                });
        }

        if !self.snackbars.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ensure_examples_current();
        self.poll_runtime_logs();

        if self.pending_hot_reload_run {
            self.pending_hot_reload_run = false;
            self.run_selected_example();
        }

        egui::TopBottomPanel::bottom("console_panel")
            .resizable(true)
            .default_height(180.0)
            .show(ctx, |ui| self.console_ui(ui, ctx));

        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(240.0)
            .show(ctx, |ui| self.sidebar_ui(ui));

        egui::CentralPanel::default().show(ctx, |ui| self.main_panel_ui(ui, ctx));

        self.show_snackbars(ctx);
    }
}

#[derive(Clone)]
struct ExampleListEntry {
    id: String,
    title: String,
    note: Option<String>,
}

#[derive(Clone)]
struct ConsoleEntry {
    kind: ConsoleKind,
    message: String,
}

impl ConsoleEntry {
    fn new(kind: ConsoleKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn info(message: impl Into<String>) -> Self {
        Self::new(ConsoleKind::Info, message)
    }

    fn stdout(message: impl Into<String>) -> Self {
        Self::new(ConsoleKind::Stdout, message)
    }

    fn stderr(message: impl Into<String>) -> Self {
        Self::new(ConsoleKind::Stderr, message)
    }

    fn result(message: impl Into<String>) -> Self {
        Self::new(ConsoleKind::Result, message)
    }

    fn error(message: impl Into<String>) -> Self {
        Self::new(ConsoleKind::Error, message)
    }

    fn log(message: impl Into<String>) -> Self {
        Self::new(ConsoleKind::Log, message)
    }
}

#[derive(Clone, Copy)]
enum ConsoleKind {
    Info,
    Stdout,
    Stderr,
    Result,
    Error,
    Log,
}

impl ConsoleKind {
    fn color(self, visuals: &egui::Visuals) -> Color32 {
        match self {
            Self::Info => visuals.text_color(),
            Self::Stdout => Color32::from_rgb(120, 200, 120),
            Self::Stderr => Color32::from_rgb(220, 100, 100),
            Self::Result => Color32::from_rgb(120, 180, 240),
            Self::Error => Color32::from_rgb(240, 100, 120),
            Self::Log => visuals.text_color().gamma_multiply(0.8),
        }
    }
}

struct ExecutionSummary {
    duration: Duration,
    return_value: Option<String>,
    succeeded: bool,
}

struct Snackbar {
    message: String,
    kind: SnackbarKind,
    created: Instant,
    duration: Duration,
}

#[derive(Clone, Copy)]
enum SnackbarKind {
    Success,
    Error,
    Info,
}

impl SnackbarKind {
    fn color(self, visuals: &egui::Visuals) -> Color32 {
        match self {
            Self::Success => Color32::from_rgb(120, 200, 120),
            Self::Error => Color32::from_rgb(220, 100, 100),
            Self::Info => visuals.text_color(),
        }
    }
}
