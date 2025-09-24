# Example Library Layout

Koto Learning loads runnable examples from the `examples/` directory that ships with the binary. Each example lives inside its own folder and must provide at least a script, metadata, and documentation:

```
examples/
  ├─ basics/
  │   ├─ script.koto
  │   ├─ meta.json
  │   └─ docs.md
  └─ testing/
      ├─ script.koto
      ├─ meta.json
      ├─ docs.md
      └─ logs/
          └─ test_run.log
```

## `meta.json` schema

The metadata file is parsed as JSON with the following fields:

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Unique identifier for the example. Defaults to the folder name when omitted. |
| `title` | string | Human friendly title displayed in the UI. |
| `description` | string | Short summary of what the example demonstrates. |
| `note` | string (optional) | Additional text shown alongside the description. |
| `doc_url` | string (optional) | A relative path (e.g. `examples/basics/docs.md`) that points to the bundled documentation. |
| `run_instructions` | string (optional) | Step-by-step guidance for running or modifying the example. |
| `categories` | array of strings | Tags used for filtering/grouping inside the explorer UI. Empty by default. |
| `documentation` | array of objects | Additional external links rendered under “Resources”. |
| `how_it_works` | array of strings | Bullet points rendered in the UI explaining the implementation. |
| `inputs` | array of objects | Optional input controls exposed to the UI. |
| `benchmarks` / `tests` | object (optional) | Extra resources that link to benchmark or test artifacts. |

## `script.koto`

The `script.koto` file contains the Koto source code that should be evaluated when the example is run. Files are read using UTF-8 encoding.

## `docs.md`

`docs.md` is a short, task-focused explanation for the example. The loader extracts the first paragraph to show a summary in the UI and exposes a link to the full markdown file on disk.

## Logs and fixtures

Examples can include a `logs/` subfolder containing sample output or fixtures. These files are surfaced via the documentation so readers know what to expect when they run the scripts.

## Hot reloading

`koto_learning` watches the `examples/` tree at runtime using `notify`. Any edits to `meta.json`, `script.koto`, or `docs.md` files automatically trigger a reload of the in-memory example catalogue. Changes become visible in the UI without restarting the application.

Set the `KOTO_EXAMPLES_DIR` environment variable to point to an alternative examples directory when testing or developing.
