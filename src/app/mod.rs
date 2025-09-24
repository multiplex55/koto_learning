use crate::{
    examples::{self, Example},
    runtime,
};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, Grid, RichText};
use egui_extras::syntax_highlighting;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
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
    active_console_pane: ConsolePane,
    test_runs: HashMap<String, examples::tests::TestSuiteResult>,
    hot_reload_notices: Vec<HotReloadNotice>,
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
            active_console_pane: ConsolePane::Console,
            test_runs: HashMap::new(),
            hot_reload_notices: Vec::new(),
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
            let changes = library.take_recent_changes();
            if !changes.is_empty() {
                self.handle_script_changes(changes);
            }
        }
    }

    fn handle_script_changes(&mut self, changes: Vec<examples::ScriptChange>) {
        for change in changes {
            self.on_script_change(&change);
            self.hot_reload_notices.push(HotReloadNotice { change });
        }
        self.prune_hot_reload_notices();
    }

    fn on_script_change(&mut self, change: &examples::ScriptChange) {
        match &change.kind {
            examples::ScriptChangeKind::ScriptUpdated { .. } => {
                let prefix = format!("{}::", change.example_id);
                self.test_runs.retain(|key, _| !key.starts_with(&prefix));
            }
            examples::ScriptChangeKind::TestSuiteUpdated { suite_id, .. } => {
                let key = format!("{}::{suite_id}", change.example_id);
                self.test_runs.remove(&key);
            }
        }

        let message = describe_change(change);
        self.push_console_entry(ConsoleEntry::log(message.clone()));
        self.push_snackbar(message, SnackbarKind::Info);
    }

    fn prune_test_runs(&mut self) {
        let valid: HashSet<String> = self
            .examples
            .iter()
            .flat_map(|example| {
                example
                    .test_suites
                    .iter()
                    .map(move |suite| format!("{}::{}", example.metadata.id, suite.id))
            })
            .collect();
        self.test_runs.retain(|key, _| valid.contains(key));
    }

    fn prune_hot_reload_notices(&mut self) {
        let valid_examples: HashSet<_> = self
            .examples
            .iter()
            .map(|example| example.metadata.id.clone())
            .collect();
        self.hot_reload_notices
            .retain(|notice| valid_examples.contains(&notice.change.example_id));
        if self.hot_reload_notices.len() > 20 {
            let excess = self.hot_reload_notices.len() - 20;
            self.hot_reload_notices.drain(0..excess);
        }
    }

    fn refresh_examples_from_library(&mut self) {
        if let Some(library) = self.example_library {
            if let Err(error) = library.refresh() {
                self.push_console_entry(ConsoleEntry::error(format!(
                    "Failed to refresh examples: {error}"
                )));
                self.push_snackbar("Failed to refresh examples", SnackbarKind::Error);
                return;
            }

            self.examples = library.snapshot();
            self.examples_version = library.version();
            self.on_examples_changed(false);
            let changes = library.take_recent_changes();
            if !changes.is_empty() {
                self.handle_script_changes(changes);
            }
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

        self.prune_test_runs();
        self.prune_hot_reload_notices();
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

            if let Some(docs) = &example.docs {
                ui.add_space(6.0);
                ui.label(&docs.summary);
                let link_target = example
                    .metadata
                    .doc_url
                    .clone()
                    .unwrap_or_else(|| format!("file://{}", docs.path.display()));
                ui.hyperlink_to("Open detailed guide", link_target);
            } else if let Some(doc_url) = &example.metadata.doc_url {
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

            self.hot_reload_notice_ui(ui, &example);

            if example.metadata.benchmarks.is_some() || example.benchmark_summary.is_some() {
                ui.add_space(6.0);
                self.benchmark_summary_ui(ui, &example);
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

    fn benchmark_summary_ui(&self, ui: &mut egui::Ui, example: &Example) {
        ui.group(|ui| {
            ui.heading("Benchmarks");
            if let Some(summary) = &example.benchmark_summary {
                if summary.measurements.is_empty() {
                    ui.label("Run `cargo bench` to generate Criterion results for this example.");
                } else {
                    let grid_id = format!("benchmark_summary_{}", summary.example_id);
                    Grid::new(grid_id).striped(true).show(ui, |grid| {
                        grid.label(RichText::new("Implementation").strong());
                        grid.label(RichText::new("Input").strong());
                        grid.label(RichText::new("Mean (ms)").strong());
                        grid.label(RichText::new("CI (ms)").strong());
                        grid.end_row();

                        for measurement in &summary.measurements {
                            grid.label(&measurement.benchmark_id);
                            grid.label(measurement.parameter.as_deref().unwrap_or("—"));

                            let mean_response =
                                grid.label(format!("{:.3}", measurement.mean.point_estimate_ms));
                            if let Some(std_dev) = measurement.std_dev_ms {
                                mean_response.on_hover_text(format!("Std dev: {:.3} ms", std_dev));
                            }

                            let ci_text = format!(
                                "{:.3} – {:.3}",
                                measurement.mean.lower_bound_ms, measurement.mean.upper_bound_ms
                            );
                            let ci_response = grid.label(ci_text);
                            let confidence_pct = measurement.mean.confidence_level * 100.0;
                            ci_response
                                .on_hover_text(format!("{confidence_pct:.1}% confidence interval"));

                            grid.end_row();
                        }
                    });
                }

                if let Some(report_url) = &summary.report_url {
                    ui.add_space(4.0);
                    ui.hyperlink_to("Open full Criterion report", report_url);
                }
            } else {
                ui.label("Run `cargo bench` to generate Criterion results for this example.");
            }

            if let Some(resource) = &example.metadata.benchmarks {
                if let Some(description) = &resource.description {
                    ui.add_space(4.0);
                    ui.label(description);
                }
                if let Some(url) = &resource.url {
                    let link_label = resource
                        .label
                        .as_deref()
                        .unwrap_or("View benchmark artifacts");
                    ui.hyperlink_to(link_label, url);
                }
            }
        });
    }

    fn console_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.active_console_pane,
                ConsolePane::Console,
                "Console",
            );
            ui.selectable_value(&mut self.active_console_pane, ConsolePane::Tests, "Tests");
            if matches!(self.active_console_pane, ConsolePane::Console) {
                if ui.button("Copy").clicked() {
                    let text = self
                        .console_entries
                        .iter()
                        .map(|entry| entry.message.clone())
                        .collect::<Vec<_>>()
                        .join("\n");
                    ctx.copy_text(text);
                }
                if ui.button("Clear").clicked() {
                    self.console_entries.clear();
                }
            }
        });
        ui.separator();

        match self.active_console_pane {
            ConsolePane::Console => {
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
            ConsolePane::Tests => {
                self.tests_ui(ui);
            }
        }
    }

    fn tests_ui(&mut self, ui: &mut egui::Ui) {
        let Some(example) = self.selected_example().cloned() else {
            ui.label("Select an example to inspect its test suites.");
            return;
        };

        if example.test_suites.is_empty() {
            ui.label("This example doesn't define any Koto test suites yet.");
            return;
        }

        if ui.button("Run all suites").clicked() {
            self.run_all_suites(&example);
        }
        ui.separator();

        for suite in &example.test_suites {
            let key = format!("{}::{}", example.metadata.id, suite.id);
            let result = self.test_runs.get(&key).cloned();
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.heading(&suite.name);
                    if ui.button("Run").clicked() {
                        self.run_suite_for_example(&example, suite);
                    }
                });
                if let Some(description) = &suite.description {
                    ui.label(description);
                }

                if let Some(result) = result.as_ref() {
                    let status_text = if result.passed {
                        RichText::new("All tests passed").color(Color32::from_rgb(120, 200, 120))
                    } else {
                        RichText::new("Failures detected").color(Color32::from_rgb(220, 100, 100))
                    };
                    ui.label(status_text);
                    ui.label(format!(
                        "Suites: {} tests, {} ms total",
                        result.cases.len(),
                        result.total_duration.as_millis()
                    ));

                    if !result.setup_stdout.is_empty() {
                        ui.collapsing("Suite stdout", |ui| {
                            ui.monospace(&result.setup_stdout);
                        });
                    }
                    if !result.setup_stderr.is_empty() {
                        ui.collapsing("Suite stderr", |ui| {
                            ui.monospace(&result.setup_stderr);
                        });
                    }

                    for case in &result.cases {
                        let header = egui::CollapsingHeader::new(format!(
                            "{} ({:.0} ms)",
                            case.name,
                            case.duration.as_secs_f32() * 1000.0
                        ))
                        .default_open(matches!(case.status, examples::tests::TestStatus::Failed));

                        header.show(ui, |ui| {
                            let status =
                                match case.status {
                                    examples::tests::TestStatus::Passed => RichText::new("Passed")
                                        .color(Color32::from_rgb(120, 200, 120)),
                                    examples::tests::TestStatus::Failed => RichText::new("Failed")
                                        .color(Color32::from_rgb(220, 100, 100)),
                                };
                            ui.label(status);
                            if let Some(error) = &case.error {
                                ui.label(
                                    RichText::new(error).color(Color32::from_rgb(220, 100, 100)),
                                );
                            }
                            if !case.stdout.is_empty() {
                                ui.collapsing("Stdout", |ui| ui.monospace(&case.stdout));
                            }
                            if !case.stderr.is_empty() {
                                ui.collapsing("Stderr", |ui| ui.monospace(&case.stderr));
                            }
                        });
                    }
                } else {
                    ui.label("Run the suite to view results.");
                }
            });
        }
    }

    fn run_suite_for_example(
        &mut self,
        example: &Example,
        suite: &examples::tests::ExampleTestSuite,
    ) {
        let key = format!("{}::{}", example.metadata.id, suite.id);
        self.active_console_pane = ConsolePane::Tests;
        self.push_console_entry(ConsoleEntry::info(format!(
            "Running suite '{}' for '{}'",
            suite.name, example.metadata.title
        )));

        match examples::tests::run_suite(suite) {
            Ok(result) => {
                let passed_count = result
                    .cases
                    .iter()
                    .filter(|case| case.status == examples::tests::TestStatus::Passed)
                    .count();
                let message = format!(
                    "Suite '{}' finished: {passed_count}/{} cases passed ({} ms)",
                    suite.name,
                    result.cases.len(),
                    result.total_duration.as_millis()
                );
                if result.passed {
                    self.push_console_entry(ConsoleEntry::info(message.clone()));
                    self.push_snackbar(message, SnackbarKind::Success);
                } else {
                    self.push_console_entry(ConsoleEntry::error(message.clone()));
                    self.push_snackbar(message, SnackbarKind::Error);
                }
                self.test_runs.insert(key, result);
            }
            Err(error) => {
                self.push_console_entry(ConsoleEntry::error(format!(
                    "Failed to run suite '{}': {error}",
                    suite.name
                )));
                self.push_snackbar("Test suite failed to run", SnackbarKind::Error);
                self.test_runs.remove(&key);
            }
        }
    }

    fn run_all_suites(&mut self, example: &Example) {
        if example.test_suites.is_empty() {
            return;
        }

        self.active_console_pane = ConsolePane::Tests;
        self.push_console_entry(ConsoleEntry::info(format!(
            "Running {} suites for '{}'",
            example.test_suites.len(),
            example.metadata.title
        )));

        let mut any_failed = false;
        for suite in &example.test_suites {
            self.run_suite_for_example(example, suite);
            let key = format!("{}::{}", example.metadata.id, suite.id);
            if let Some(result) = self.test_runs.get(&key) {
                if !result.passed {
                    any_failed = true;
                }
            }
        }

        let summary = if any_failed {
            format!(
                "Finished running suites for '{}' with failures",
                example.metadata.title
            )
        } else {
            format!("All suites for '{}' passed", example.metadata.title)
        };

        if any_failed {
            self.push_console_entry(ConsoleEntry::error(summary.clone()));
            self.push_snackbar(summary, SnackbarKind::Error);
        } else {
            self.push_console_entry(ConsoleEntry::info(summary.clone()));
            self.push_snackbar(summary, SnackbarKind::Success);
        }
    }

    fn hot_reload_notice_ui(&mut self, ui: &mut egui::Ui, example: &Example) {
        let notices: Vec<_> = self
            .hot_reload_notices
            .iter()
            .enumerate()
            .filter(|(_, notice)| notice.change.example_id == example.metadata.id)
            .map(|(index, notice)| (index, notice.clone()))
            .collect();

        if notices.is_empty() {
            return;
        }

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.heading("Hot reload updates");
            ui.label("Changes were detected on disk. Re-run the example or revert below.");

            let mut to_remove = Vec::new();

            for (index, notice) in notices {
                ui.separator();
                let description = describe_change(&notice.change);
                let elapsed = notice
                    .change
                    .changed_at
                    .elapsed()
                    .map(format_elapsed)
                    .unwrap_or_else(|_| "just now".to_string());
                let file_name = notice
                    .change
                    .path
                    .file_name()
                    .and_then(|name| name.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| notice.change.path.to_string_lossy().into_owned());

                ui.vertical(|ui| {
                    ui.label(RichText::new(description).strong());
                    ui.label(RichText::new(format!("{} • {}", file_name, elapsed)).small());
                });

                ui.horizontal(|ui| {
                    if ui.button("Revert change").clicked() {
                        if self.revert_script_change(&notice.change) {
                            to_remove.push(index);
                        }
                    }
                    if ui.button("Dismiss").clicked() {
                        to_remove.push(index);
                    }
                });
            }

            to_remove.sort_unstable();
            to_remove.dedup();
            for index in to_remove.into_iter().rev() {
                self.hot_reload_notices.remove(index);
            }
        });
    }

    fn revert_script_change(&mut self, change: &examples::ScriptChange) -> bool {
        let Some(library) = self.example_library else {
            self.push_console_entry(ConsoleEntry::error(
                "Example library is unavailable; cannot revert change",
            ));
            self.push_snackbar("Revert not available", SnackbarKind::Error);
            return false;
        };

        match library.revert_change(change) {
            Ok(_) => {
                self.push_console_entry(ConsoleEntry::info(format!(
                    "Reverted change: {}",
                    describe_change(change)
                )));

                if let Err(error) = library.refresh() {
                    self.push_console_entry(ConsoleEntry::error(format!(
                        "Failed to reload examples after revert: {error}",
                    )));
                    self.push_snackbar("Revert applied with reload errors", SnackbarKind::Error);
                } else {
                    // Refresh local snapshot and discard any reload notices created by the revert.
                    self.examples = library.snapshot();
                    self.examples_version = library.version();
                    self.on_examples_changed(false);
                    let _ = library.take_recent_changes();
                    self.push_snackbar("Change reverted", SnackbarKind::Success);
                }
                true
            }
            Err(error) => {
                self.push_console_entry(ConsoleEntry::error(format!(
                    "Failed to revert change: {error}",
                )));
                self.push_snackbar("Revert failed", SnackbarKind::Error);
                false
            }
        }
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConsolePane {
    Console,
    Tests,
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

#[derive(Clone)]
struct HotReloadNotice {
    change: examples::ScriptChange,
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

fn describe_change(change: &examples::ScriptChange) -> String {
    let action = match &change.kind {
        examples::ScriptChangeKind::ScriptUpdated { previous, current } => change_action(
            "script",
            change,
            previous.is_some(),
            current.is_some(),
            None,
        ),
        examples::ScriptChangeKind::TestSuiteUpdated {
            suite_id,
            previous,
            current,
        } => change_action(
            "test suite",
            change,
            previous.is_some(),
            current.is_some(),
            Some(suite_id),
        ),
    };
    action
}

fn change_action(
    kind: &str,
    change: &examples::ScriptChange,
    had_previous: bool,
    has_current: bool,
    suite: Option<&str>,
) -> String {
    let verb = match (had_previous, has_current) {
        (false, true) => "added",
        (true, true) => "updated",
        (true, false) => "removed",
        (false, false) => "updated",
    };

    let file_name = change
        .path
        .file_name()
        .and_then(|name| name.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| change.path.to_string_lossy().into_owned());

    match suite {
        Some(suite_id) => format!(
            "Example '{}' {kind} '{suite_id}' {verb} ({})",
            change.example_id, file_name
        ),
        None => format!(
            "Example '{}' {kind} {verb} ({})",
            change.example_id, file_name
        ),
    }
}

fn format_elapsed(duration: Duration) -> String {
    if duration.as_secs() >= 3600 {
        let hours = duration.as_secs() / 3600;
        let minutes = (duration.as_secs() % 3600) / 60;
        if minutes == 0 {
            format!("{hours}h ago")
        } else {
            format!("{hours}h {minutes}m ago")
        }
    } else if duration.as_secs() >= 60 {
        let minutes = duration.as_secs() / 60;
        let seconds = duration.as_secs() % 60;
        if seconds == 0 {
            format!("{minutes}m ago")
        } else {
            format!("{minutes}m {seconds}s ago")
        }
    } else if duration.as_millis() >= 1000 {
        format!("{}s ago", duration.as_secs())
    } else {
        format!("{}ms ago", duration.as_millis())
    }
}
