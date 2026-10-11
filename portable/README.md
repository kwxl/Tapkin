# Windows portable build

Extract the entire ZIP into a writable folder and run `Tapkin.exe`.
Microsoft Edge WebView2 Runtime must be installed separately.

`portable.json` (version 1) beside the executable enables portable mode. Without
this marker, Tapkin uses the normal installed application's storage locations.
The current working directory does not affect portable path resolution.

- `settings.json` contains the initial defaults and stores subsequent changes.
- `skins/example/` contains the bundled editable skin. Missing bundled files are
  restored without overwriting existing files.
- `webview-data/` is created at runtime for both application WebViews.

Move the whole folder to keep settings and internal skins. Internal skin paths
are saved relative to this folder. Put new skin folders directly in `skins/`;
Choose skin lists only those folders, and Open skin folder opens this repository.
External skin folders are not selectable. A missing or invalid selected skin falls back to
the bundled example. Missing or invalid settings use application defaults.

Portable mode does not import installed settings or modify existing launch-at-login
registrations. Launch at login is unavailable in portable mode. An unwritable
portable folder is an error; the application does not silently use AppData instead.