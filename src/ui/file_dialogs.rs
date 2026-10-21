//! Native dialog rules from the desktop frontend and its pinned Wails runtime.
use anyhow::Result;
use chrono::{DateTime, Local};
use futures::channel::oneshot;
use gpui::{App, Window};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    OpenConnectionBackup,
    SaveConnectionBackup,
    OpenCsv,
    SaveCsv,
    OpenFile,
    SaveMonitorLog,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Request {
    pub save: bool,
    pub extensions: &'static [&'static str],
    pub filename: String,
    pub show_hidden: bool,
}

impl Request {
    pub fn new(purpose: Purpose, at: DateTime<Local>) -> Self {
        let stamp = at.format("%Y%m%d%H%M%S");
        let (save, extensions, filename, show_hidden): (_, &'static [&'static str], _, _) =
            match purpose {
                Purpose::OpenConnectionBackup => (false, &["zip"], String::new(), true),
                Purpose::SaveConnectionBackup => {
                    (true, &["zip"], format!("connections_{stamp}.zip"), true)
                }
                Purpose::OpenCsv => (false, &["csv"], String::new(), true),
                Purpose::SaveCsv => (true, &["csv"], format!("export_{stamp}.csv"), true),
                Purpose::OpenFile => (false, &[], String::new(), true),
                Purpose::SaveMonitorLog => {
                    (true, &["txt"], format!("monitor_log_{stamp}.txt"), false)
                }
            };
        Self {
            save,
            extensions,
            filename,
            show_hidden,
        }
    }
}

/// ExportKeyDialog computes the default path when its file slot renders.
/// An empty cancellation leaves the existing default until the path changes.
pub(super) struct CsvFilename {
    path: String,
    pub name: String,
}
impl CsvFilename {
    pub fn new(at: DateTime<Local>) -> Self {
        Self {
            path: String::new(),
            name: Request::new(Purpose::SaveCsv, at).filename,
        }
    }
    pub fn path_changed(&mut self, path: String, at: DateTime<Local>) {
        if self.path != path {
            self.path = path;
            self.name = Request::new(Purpose::SaveCsv, at).filename;
        }
    }
}

/// FileOpenInput preserves the value on RPC failure; FileSaveInput clears it.
/// Both receive a successful empty path when the native panel is cancelled.
pub(super) fn field_value(result: Result<Option<PathBuf>>, save: bool) -> Option<String> {
    match result {
        Ok(path) => Some(
            path.map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        Err(_) if save => Some(String::new()),
        Err(_) => None,
    }
}

pub(super) fn cancelled_backup_error() -> anyhow::Error {
    // ExportConnections and ImportConnections pass the cancelled empty path to
    // os.Create / zip.OpenReader. Keep the original Go PathError for this case.
    #[cfg(unix)]
    let message = "open : no such file or directory";
    #[cfg(windows)]
    let message = "open : The system cannot find the file specified.";
    anyhow::anyhow!(message)
}

#[cfg(any(test, feature = "ui-qa"))]
fn source_requests() -> Vec<(Request, serde_json::Value)> {
    use chrono::TimeZone;
    let source: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/file-dialog-source.json")).unwrap();
    let time = chrono::DateTime::parse_from_rfc3339(source["fixedLocalTime"].as_str().unwrap())
        .unwrap()
        .naive_local();
    let at = Local.from_local_datetime(&time).single().unwrap();
    source["records"]
        .as_array()
        .unwrap()
        .iter()
        .take(6)
        .map(|record| {
            let purpose = match record["name"].as_str().unwrap() {
                "OpenConnectionBackup" => Purpose::OpenConnectionBackup,
                "SaveConnectionBackup" => Purpose::SaveConnectionBackup,
                "OpenCsv" => Purpose::OpenCsv,
                "SaveCsv" => Purpose::SaveCsv,
                "OpenFile" => Purpose::OpenFile,
                "SaveMonitorLog" => Purpose::SaveMonitorLog,
                name => panic!("unknown source request {name}"),
            };
            (Request::new(purpose, at), record.clone())
        })
        .collect()
}

#[cfg(all(target_os = "macos", feature = "ui-qa"))]
pub(super) fn verify_native_panels(output: &std::path::Path) {
    macos::verify_native_panels(output);
}

pub(super) fn prompt(
    request: Request,
    window: &Window,
    cx: &App,
) -> oneshot::Receiver<Result<Option<PathBuf>>> {
    #[cfg(target_os = "macos")]
    {
        macos::prompt(request, window, cx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // GPUI's other platform APIs do not expose filter or hidden-file options.
        // Keep this fallback explicit until those backends can honor the full request.
        let _ = window;
        let (sender, receiver) = oneshot::channel();
        if request.save {
            let pending = cx.prompt_for_new_path(std::path::Path::new(""), Some(&request.filename));
            cx.background_executor()
                .spawn(async move {
                    let result = pending
                        .await
                        .map_err(anyhow::Error::from)
                        .and_then(|result| result);
                    let _ = sender.send(result);
                })
                .detach();
        } else {
            let pending = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: None,
            });
            cx.background_executor()
                .spawn(async move {
                    let result = pending
                        .await
                        .map_err(anyhow::Error::from)
                        .and_then(|result| result)
                        .map(|paths| paths.and_then(|paths| paths.into_iter().next()));
                    let _ = sender.send(result);
                })
                .detach();
        }
        receiver
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::RcBlock;
    use objc2::{ClassType, MainThreadMarker, rc::Retained};
    use objc2_app_kit::{
        NSModalResponse, NSModalResponseOK, NSOpenPanel, NSSavePanel, NSView, NSWindow,
    };
    use objc2_foundation::{NSArray, NSString, NSURL};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::cell::Cell;

    fn parent(window: &Window) -> Result<Retained<NSWindow>> {
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            anyhow::bail!("file dialog requires an AppKit window");
        };
        // GPUI guarantees the borrowed NSView lives for this Window handle.
        // Retain its parent before deferring native presentation to the UI executor.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        view.window()
            .ok_or_else(|| anyhow::anyhow!("file dialog window is closed"))
    }

    #[allow(deprecated)] // Same macOS 11-compatible filter API used by Wails 2.13.0.
    fn configure(panel: &NSSavePanel, request: &Request) {
        panel.setTitle(Some(&NSString::new()));
        panel.setNameFieldStringValue(&NSString::from_str(&request.filename));
        // Wails passes an empty DefaultDirectory through fileURLWithPath:. It may
        // return nil despite the SDK's nonnull annotation (observed on macOS 26).
        // Receive that native optional value instead of using the nonnull binding.
        let directory: Option<Retained<NSURL>> =
            unsafe { objc2::msg_send![NSURL::class(), fileURLWithPath: &*NSString::new()] };
        panel.setDirectoryURL(directory.as_deref());
        panel.setCanCreateDirectories(false);
        panel.setTreatsFilePackagesAsDirectories(false);
        panel.setShowsHiddenFiles(request.show_hidden);
        if request.extensions.is_empty() {
            panel.setAllowedFileTypes(None);
            panel.setAllowsOtherFileTypes(true);
        } else {
            let types = request
                .extensions
                .iter()
                .map(|extension| NSString::from_str(extension))
                .collect::<Vec<_>>();
            panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&types)));
            panel.setAllowsOtherFileTypes(false);
        }
        if request.save {
            panel.setExtensionHidden(false);
            panel.setCanSelectHiddenExtension(true);
        }
    }

    fn open_panel(request: &Request, marker: MainThreadMarker) -> Retained<NSOpenPanel> {
        let panel = NSOpenPanel::openPanel(marker);
        configure(&panel, request);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        panel.setResolvesAliases(false);
        panel
    }

    #[cfg(feature = "ui-qa")]
    #[allow(deprecated)]
    pub(super) fn verify_native_panels(output: &std::path::Path) {
        let marker = MainThreadMarker::new().expect("native dialog QA runs on UI thread");
        let source: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/file-dialog-source.json"))
                .unwrap();
        let mut records = Vec::new();
        for (request, record) in source_requests() {
            let options = &record["options"];
            let native = source["native"]["records"]
                .as_array()
                .unwrap()
                .iter()
                .find(|native| native["name"] == record["name"])
                .unwrap();
            let panel: Retained<NSSavePanel> = if request.save {
                let panel = NSSavePanel::savePanel(marker);
                configure(&panel, &request);
                assert_eq!(
                    panel.isExtensionHidden(),
                    source["wails"]["extensionHidden"].as_bool().unwrap()
                );
                assert_eq!(
                    panel.canSelectHiddenExtension(),
                    source["wails"]["canSelectHiddenExtension"]
                        .as_bool()
                        .unwrap()
                );
                panel
            } else {
                let panel = open_panel(&request, marker);
                assert_eq!(
                    panel.canChooseFiles(),
                    source["wails"]["files"].as_bool().unwrap()
                );
                assert_eq!(
                    panel.canChooseDirectories(),
                    source["wails"]["directories"].as_bool().unwrap()
                );
                assert_eq!(
                    panel.allowsMultipleSelection(),
                    source["wails"]["multiple"].as_bool().unwrap()
                );
                assert_eq!(
                    panel.resolvesAliases(),
                    options["ResolvesAliases"].as_bool().unwrap()
                );
                Retained::into_super(panel)
            };
            let extensions = panel
                .allowedFileTypes()
                .map(|types| types.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                .unwrap_or_default();
            let expected = options["Filters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|filter| {
                    filter["Pattern"]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("*.")
                        .to_string()
                })
                .collect::<Vec<_>>();
            assert_eq!(extensions, expected, "{} native filters", record["name"]);
            assert_eq!(panel.allowsOtherFileTypes(), extensions.is_empty());
            assert_eq!(panel.title().to_string(), native["title"].as_str().unwrap());
            assert_eq!(
                panel.nameFieldStringValue().to_string(),
                native["filename"].as_str().unwrap()
            );
            assert_eq!(
                panel.showsHiddenFiles(),
                options["ShowHiddenFiles"].as_bool().unwrap()
            );
            assert_eq!(
                panel.canCreateDirectories(),
                options["CanCreateDirectories"].as_bool().unwrap()
            );
            assert_eq!(
                panel.treatsFilePackagesAsDirectories(),
                options["TreatPackagesAsDirectories"].as_bool().unwrap()
            );
            let directory = panel.directoryURL().and_then(|url| url.to_file_path());
            // Empty DefaultDirectory uses AppKit's default/last location. Its
            // exact directory belongs to the OS and is not fixed by the app.
            assert_eq!(
                directory.is_none(),
                native["directoryIsNil"].as_bool().unwrap()
            );
            records.push(serde_json::json!({"name":record["name"],"filename":panel.nameFieldStringValue().to_string(),"extensions":extensions,"showHidden":panel.showsHiddenFiles(),"canCreateDirectories":panel.canCreateDirectories(),"directoryIsNil":directory.is_none()}));
        }
        std::fs::write(
            output.join("file-dialog-panels.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
        eprintln!(
            "FILE DIALOG QA PASS: {} native panel configurations",
            records.len()
        );
    }

    pub fn prompt(
        request: Request,
        window: &Window,
        cx: &App,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (sender, receiver) = oneshot::channel();
        let parent = parent(window);
        cx.foreground_executor()
            .spawn(async move {
                let parent = match parent {
                    Ok(parent) => parent,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        return;
                    }
                };
                let Some(marker) = MainThreadMarker::new() else {
                    let _ = sender.send(Err(anyhow::anyhow!(
                        "file dialog must run on the UI thread"
                    )));
                    return;
                };
                if request.save {
                    let panel = NSSavePanel::savePanel(marker);
                    configure(&panel, &request);
                    let sender = Cell::new(Some(sender));
                    let handler = RcBlock::new({
                        let panel = panel.clone();
                        move |response: NSModalResponse| {
                            if let Some(sender) = sender.take() {
                                let path = (response == NSModalResponseOK)
                                    .then(|| panel.URL().and_then(|url| url.to_file_path()))
                                    .flatten();
                                let _ = sender.send(Ok(path));
                            }
                        }
                    });
                    panel.beginSheetModalForWindow_completionHandler(&parent, &handler);
                } else {
                    let panel = open_panel(&request, marker);
                    let sender = Cell::new(Some(sender));
                    let handler = RcBlock::new({
                        let panel = panel.clone();
                        move |response: NSModalResponse| {
                            if let Some(sender) = sender.take() {
                                let path = (response == NSModalResponseOK)
                                    .then(|| {
                                        panel
                                            .URLs()
                                            .firstObject()
                                            .and_then(|url| url.to_file_path())
                                    })
                                    .flatten();
                                let _ = sender.send(Ok(path));
                            }
                        }
                    });
                    panel.beginSheetModalForWindow_completionHandler(&parent, &handler);
                }
            })
            .detach();
        receiver
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_dialog_requests_match_original_services() {
        for (request, record) in source_requests() {
            let options = &record["options"];
            assert_eq!(request.save, record["save"].as_bool().unwrap());
            assert_eq!(
                request.filename,
                options["DefaultFilename"].as_str().unwrap()
            );
            assert_eq!(
                request.show_hidden,
                options["ShowHiddenFiles"].as_bool().unwrap()
            );
            let expected = options["Filters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|filter| filter["Pattern"].as_str().unwrap().trim_start_matches("*."))
                .collect::<Vec<_>>();
            assert_eq!(request.extensions, expected);
            if cfg!(unix)
                && record["name"]
                    .as_str()
                    .unwrap()
                    .ends_with("ConnectionBackup")
            {
                assert_eq!(
                    cancelled_backup_error().to_string(),
                    record["result"]["msg"].as_str().unwrap()
                );
            }
        }
    }

    #[test]
    fn csv_filename_matches_original_slot_render_times() {
        use chrono::TimeZone;
        let source: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/csv-filename-source.json"
        ))
        .unwrap();
        let at = |minute| {
            Local
                .with_ymd_and_hms(2026, 10, 15, 13, minute, 0)
                .single()
                .unwrap()
        };
        let expected = |index: usize| source["records"][index]["filename"].as_str().unwrap();
        let mut cache = CsvFilename::new(at(0));
        assert_eq!(cache.name, expected(0));
        cache.path_changed(String::new(), at(1));
        assert_eq!(cache.name, expected(1));
        cache.path_changed(String::new(), at(2));
        cache.path_changed("/tmp/typed.csv".into(), at(3));
        assert_eq!(cache.name, expected(2));
        cache.path_changed(String::new(), at(4));
        assert_eq!(cache.name, expected(3));
        cache.path_changed(String::new(), at(6));
        assert_eq!(cache.name, expected(3));
        cache = CsvFilename::new(at(7));
        assert_eq!(cache.name, expected(4));
    }

    #[test]
    fn file_dialog_results_match_original_vue_inputs() {
        let source: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/file-dialog-source.json"))
                .unwrap();
        for record in source["frontend"]["records"].as_array().unwrap() {
            let response = &record["response"];
            let result = if response["success"] == true {
                let path = response["data"]["path"]
                    .as_str()
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from);
                Ok(path)
            } else {
                Err(anyhow::anyhow!("fixture RPC failure"))
            };
            let actual = field_value(result, record["save"].as_bool().unwrap())
                .unwrap_or_else(|| record["before"].as_str().unwrap().to_string());
            assert_eq!(
                actual,
                record["value"].as_str().unwrap(),
                "{}",
                record["name"]
            );
        }
    }
}
