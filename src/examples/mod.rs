use std::{fs, path::PathBuf};

use anyhow::Result;
use directories::ProjectDirs;
use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Example {
    pub title: String,
    pub description: String,
    pub source: String,
}

pub fn load_examples() -> Result<Vec<Example>> {
    let mut examples = built_in_examples();

    if let Some(storage_dir) = project_data_dir() {
        let file_path = storage_dir.join("examples.json");
        if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => match serde_json::from_str::<Vec<Example>>(&content) {
                    Ok(mut user_examples) => examples.append(&mut user_examples),
                    Err(error) => warn!("Failed to parse {file_path:?}: {error}"),
                },
                Err(error) => warn!("Failed to read {file_path:?}: {error}"),
            }
        }
    }

    Ok(examples)
}

fn project_data_dir() -> Option<PathBuf> {
    ProjectDirs::from("dev", "Koto", "KotoLearning").map(|dirs| dirs.data_dir().to_path_buf())
}

fn built_in_examples() -> Vec<Example> {
    vec![
        Example {
            title: "Hello World".to_string(),
            description: "Print a traditional greeting".to_string(),
            source: "print(\"Hello, world!\")".to_string(),
        },
        Example {
            title: "Fibonacci".to_string(),
            description: "Calculate a Fibonacci number".to_string(),
            source:
                "\nfn fib(n) => if n <= 1 then n else fib(n - 1) + fib(n - 2)\nprint(fib(10))\n"
                    .to_string(),
        },
    ]
}
