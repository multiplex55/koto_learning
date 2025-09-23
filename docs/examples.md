# Example Library Layout

Koto Learning loads runnable examples from the `examples/` directory that ships
with the binary. Each example lives inside its own folder and must provide both
a script and metadata:

```
examples/
  └─ hello_world/
      ├─ script.koto
      └─ meta.json
```

## `meta.json` schema

The metadata file is parsed as JSON with the following fields:

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Unique identifier for the example. Defaults to the folder name when omitted. |
| `title` | string | Human friendly title displayed in the UI. |
| `description` | string | Short summary of what the example demonstrates. |
| `note` | string (optional) | Additional text shown alongside the description. |
| `doc_url` | string (optional) | Link to reference documentation for deeper reading. |
| `run_instructions` | string (optional) | Step-by-step guidance for running or modifying the example. |
| `categories` | array of strings | Tags used for filtering/grouping inside the explorer UI. Empty by default. |

## `script.koto`

The `script.koto` file contains the Koto source code that should be evaluated
when the example is run. Files are read using UTF-8 encoding.

## Hot reloading

`koto_learning` watches the `examples/` tree at runtime using `notify`. Any
edits to `meta.json` or `script.koto` files automatically trigger a reload of
the in-memory example catalogue. Changes become visible in the UI without
restarting the application.

Set the `KOTO_EXAMPLES_DIR` environment variable to point to an alternative
examples directory when testing or developing.
