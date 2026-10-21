//! Transfer lifetime and messages follow the original browser store, independently
//! of the form lifetime. Wails progress/stop events are scoped by kind and path.
use super::*;
use crate::service::TransferProgress;
use crate::ui::notices::MessageKind;
#[cfg(feature = "ui-qa")]
#[path = "csv_progress_qa.rs"]
mod qa;
#[cfg(feature = "ui-qa")]
pub(in crate::ui) use qa::verify as qa_verify_transfers;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum CsvKind {
    Import,
    Export,
}

pub(in crate::ui) struct CsvTransfer {
    kind: CsvKind,
    path: String,
    cancel: Cancellation,
}
pub(in crate::ui) struct CsvProgressEvent {
    kind: CsvKind,
    path: String,
    report: TransferProgress,
}
enum Outcome {
    Completed(TransferProgress),
    Failed(String),
    // A rejected RPC is swallowed by the original form's .catch(). The store
    // removes its loading message in finally and skips the completion summary.
    Rejected,
}
struct Completion {
    messages: Vec<(MessageKind, String)>,
    reload: bool,
}

fn translated(tr: &impl Fn(&str) -> String, key: &str, values: &[(&str, usize)]) -> String {
    let mut text = tr(key);
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}
fn progress_text(kind: CsvKind, report: &TransferProgress, tr: impl Fn(&str) -> String) -> String {
    match kind {
        CsvKind::Export => translated(
            &tr,
            "dialogue.export.exporting",
            &[("index", report.current), ("count", report.total)],
        ),
        CsvKind::Import => translated(
            &tr,
            "dialogue.import.importing",
            &[
                ("imported", report.done),
                ("conflict", report.failed + report.skipped),
            ],
        ),
    }
}
fn completion(
    kind: CsvKind,
    reload: bool,
    result: Outcome,
    tr: impl Fn(&str) -> String,
) -> Completion {
    let mut messages = Vec::new();
    let report = match result {
        Outcome::Completed(report) => report,
        Outcome::Failed(error) => {
            messages.push((MessageKind::Error, error));
            // Preserve the original store's zero-count summary after a normal
            // {success:false} response, including its import reload policy.
            TransferProgress::default()
        }
        Outcome::Rejected => {
            return Completion {
                messages,
                reload: false,
            };
        }
    };
    let reload = kind == CsvKind::Import && reload && !report.canceled;
    if report.canceled {
        messages.push((MessageKind::Info, tr("dialogue.handle_cancel")));
    } else {
        let (level, text) = match kind {
            CsvKind::Import => (
                MessageKind::Success,
                translated(
                    &tr,
                    "dialogue.import.import_completed",
                    &[
                        ("success", report.done),
                        ("ignored", report.failed + report.skipped),
                    ],
                ),
            ),
            CsvKind::Export => (
                if report.failed == 0 {
                    MessageKind::Success
                } else if report.failed >= report.done {
                    MessageKind::Error
                } else {
                    MessageKind::Warning
                },
                translated(
                    &tr,
                    "dialogue.export.export_completed",
                    &[("success", report.done), ("fail", report.failed)],
                ),
            ),
        };
        messages.push((level, text));
    }
    Completion { messages, reload }
}

impl RdmApp {
    pub(in crate::ui) fn submit_csv_transfer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dialog = self.dialog.as_ref().unwrap();
        let kind = if matches!(dialog.kind, DialogKind::Import) {
            CsvKind::Import
        } else {
            CsvKind::Export
        };
        let Some(target) = dialog.target.clone() else {
            return;
        };
        let path = dialog.value("path", cx);
        if path.is_empty() || (kind == CsvKind::Export && target.keys.is_empty()) {
            return;
        }
        let reload = dialog.boolean("reload");
        let replace = dialog.value("conflict", cx) == "0";
        let include_ttl = dialog.boolean("include_ttl");
        let ttl = match dialog.value("ttl_type", cx).as_str() {
            "0" => Some(-1),
            "1" => Some(0),
            _ => {
                let value = dialog.numbers.get("ttl").copied().flatten().unwrap_or(0.)
                    * dialog.value("unit", cx).parse::<f64>().unwrap_or(1.);
                (value.is_finite()
                    && value.fract() == 0.
                    && value >= i64::MIN as f64
                    && value < i64::MAX as f64)
                    .then_some(value as i64)
            }
        };
        // The form closes synchronously, before either RPC result or progress.
        self.close_dialog(window, cx);
        let message = self.csv_loading_message();
        let cancel = Cancellation::default();
        self.csv_transfers.insert(
            message,
            CsvTransfer {
                kind,
                path: path.clone(),
                cancel: cancel.clone(),
            },
        );
        let progress_sender = self.csv_progress.0.clone();
        let service = self.service.clone();
        let server = target.conn.name.clone();
        let task = cx.background_executor().spawn(async move {
            let update = |report| {
                let _ = progress_sender.send(CsvProgressEvent {
                    kind,
                    path: path.clone(),
                    report,
                });
            };
            let result = match kind {
                CsvKind::Export => service.export_csv(
                    &target.conn,
                    target.db,
                    &target.keys,
                    std::path::Path::new(&path),
                    include_ttl,
                    &cancel,
                    update,
                ),
                CsvKind::Import => {
                    let Some(ttl) = ttl else {
                        return Outcome::Rejected;
                    };
                    service.import_csv(
                        &target.conn,
                        target.db,
                        std::path::Path::new(&path),
                        replace,
                        ttl,
                        &cancel,
                        update,
                    )
                }
            };
            match result {
                Ok(report) => Outcome::Completed(report),
                Err(error) => Outcome::Failed(error.to_string()),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.finish_csv_transfer(message, kind, &server, reload, result, window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn finish_csv_transfer(
        &mut self,
        message: u64,
        kind: CsvKind,
        server: &str,
        reload: bool,
        result: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.csv_transfers.remove(&message);
        self.destroy_message(message);
        let result = completion(kind, reload, result, |key| self.tr(key));
        for (level, text) in result.messages {
            self.add_message(text, level);
        }
        if result.reload {
            // Source reloadServer targets the server's currently displayed DB,
            // including when the user switched databases during the transfer.
            if let Some(tab) = self.tabs.iter().find(|tab| tab.conn.name == server) {
                self.scan_keys(tab.id, true, false, window, cx);
            }
        }
    }
    pub(in crate::ui) fn cancel_csv_transfer(&mut self, message: u64) {
        let Some(origin) = self.csv_transfers.get(&message) else {
            return;
        };
        for transfer in self.csv_transfers.values() {
            if transfer.kind == origin.kind && transfer.path == origin.path {
                transfer.cancel.cancel();
            }
        }
    }
    pub(in crate::ui) fn poll_csv_transfers(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.csv_progress.1.try_recv() {
            let text = progress_text(event.kind, &event.report, |key| self.tr(key));
            for (&id, transfer) in &self.csv_transfers {
                if transfer.kind == event.kind && transfer.path == event.path {
                    if let Some(notice) = self.messages.iter_mut().find(|notice| notice.id == id) {
                        changed |= notice.text != text;
                        notice.text = text.clone();
                    }
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::{CsvKind, MessageKind, Outcome, TransferProgress, completion, progress_text};
    use serde_json::Value as Json;
    #[test]
    fn csv_messages_match_original_store_samples() {
        let fixture: Json = serde_json::from_str(include_str!(
            "../../../tests/fixtures/csv-progress-source.json"
        ))
        .unwrap();
        let lang: Json =
            serde_json::from_str(include_str!("../../../assets/langs/zh-cn.json")).unwrap();
        let tr = |key: &str| {
            lang.pointer(&format!("/{}", key.replace('.', "/")))
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        };
        for record in fixture["records"].as_array().unwrap() {
            let kind = if record["kind"] == "import" {
                CsvKind::Import
            } else {
                CsvKind::Export
            };
            let response = &record["response"];
            let result = if record["reject"] == true {
                Outcome::Rejected
            } else if response["success"] == false {
                Outcome::Failed(response["msg"].as_str().unwrap().into())
            } else {
                Outcome::Completed(TransferProgress {
                    done: response["data"][if kind == CsvKind::Import {
                        "imported"
                    } else {
                        "exported"
                    }]
                    .as_u64()
                    .unwrap() as usize,
                    failed: response["data"][if kind == CsvKind::Import {
                        "ignored"
                    } else {
                        "failed"
                    }]
                    .as_u64()
                    .unwrap() as usize,
                    canceled: response["data"]["canceled"].as_bool().unwrap(),
                    ..Default::default()
                })
            };
            let actual = completion(kind, record["reload"] == true, result, &tr);
            let expected = record["completed"].as_array().unwrap();
            assert_eq!(actual.messages.len(), expected.len(), "{}", record["name"]);
            for ((level, text), expected) in actual.messages.iter().zip(expected) {
                assert_eq!(
                    text,
                    expected["text"].as_str().unwrap(),
                    "{}",
                    record["name"]
                );
                let label = match level {
                    MessageKind::Success => "success",
                    MessageKind::Error => "error",
                    MessageKind::Warning => "warning",
                    MessageKind::Info => "info",
                    MessageKind::Loading => "loading",
                };
                assert!(
                    expected["classes"]
                        .as_str()
                        .unwrap()
                        .contains(&format!("n-message--{label}-type")),
                    "{}",
                    record["name"]
                );
            }
            assert_eq!(
                actual.reload,
                !record["reloads"].as_array().unwrap().is_empty(),
                "{}",
                record["name"]
            );
            let report = if kind == CsvKind::Export {
                TransferProgress {
                    current: 1,
                    total: 3,
                    ..Default::default()
                }
            } else {
                TransferProgress {
                    done: 1,
                    skipped: 2,
                    ..Default::default()
                }
            };
            assert_eq!(
                progress_text(kind, &report, &tr),
                record["progress"][0]["text"].as_str().unwrap()
            );
        }
    }
}
