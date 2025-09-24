use anyhow::{Result, anyhow};
use eframe::NativeOptions;
use koto_learning::{app::ExplorerApp, runtime::logging};

fn main() -> Result<()> {
    logging::init_global()?;
    log::info!("Launching Koto Learning Explorer");

    let native_options = NativeOptions::default();

    eframe::run_native(
        "Koto Learning Explorer",
        native_options,
        Box::new(|cc| Ok(Box::new(ExplorerApp::new(cc)))),
    )
    .map_err(|error| anyhow!("Failed to start UI: {error}"))?;

    Ok(())
}
