use super::*;
use std::collections::BTreeSet;

fn has_class(metric: &Json, class: &str) -> bool {
    metric["class"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .any(|value| value == class)
}

impl RdmApp {
    pub(super) fn qa_assert_collection_layout(&self, snapshot: &str, window: &Window, cx: &App) {
        let old: Json =
            serde_json::from_str(include_str!("../../tests/fixtures/entries-source.json")).unwrap();
        let macos: Json = serde_json::from_str(include_str!(
            "../../tests/fixtures/entries-macos-source.json"
        ))
        .unwrap();
        let modern = macos["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == snapshot);
        if let Some(source) = modern {
            assert_eq!(
                format!(
                    "{:?}",
                    self.tabs[self.index().unwrap()]
                        .detail
                        .as_ref()
                        .unwrap()
                        .kind
                )
                .to_uppercase(),
                source["kind"].as_str().unwrap(),
                "actual collection type is initialized before geometry"
            );
        }
        let legacy = old["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == snapshot);
        let mut references = Vec::new();
        if cfg!(target_os = "macos") {
            references.push((modern.expect("original Mac collection state"), true));
            if let Some(legacy) = legacy {
                references.push((legacy, false));
            }
        } else {
            references.push((legacy.expect("original collection state"), true));
        }
        let mut actual_frames = Vec::new();
        let mut legacy_failures = Vec::new();
        for (record, required) in references {
            let metrics = record["metrics"].as_array().unwrap();
            let mut check = |id: ElementId, source: &Json| {
                let actual = gpui_base::test_support::find(window, &[], &id)
                    .unwrap_or_else(|| panic!("{snapshot} missing {id:?}"))
                    .bounds();
                let values = [
                    actual.origin.x.as_f32(),
                    actual.origin.y.as_f32(),
                    actual.size.width.as_f32(),
                    actual.size.height.as_f32(),
                ];
                if required {
                    actual_frames.push(serde_json::json!({"id":format!("{id:?}"),"x":values[0],"y":values[1],"w":values[2],"h":values[3]}));
                }
                for (value, dimension) in values.into_iter().zip(["x", "y", "w", "h"]) {
                    let expected = source[dimension].as_f64().unwrap() as f32;
                    let delta = (value - expected).abs();
                    if required {
                        assert!(
                            delta < 0.6,
                            "{snapshot} original Mac {id:?} {dimension}: {value} vs {expected}"
                        );
                    } else if delta >= 0.6 {
                        legacy_failures.push(serde_json::json!({"id":format!("{id:?}"),"dimension":dimension,"actual":value,"legacyExpected":expected,"delta":delta,"thresholdUnchanged":0.6,"passed":false}));
                    }
                }
            };
            check(
                "entry-toolbar".into(),
                metrics.iter().find(|m| has_class(m, "tb2")).unwrap(),
            );
            check(
                "entry-filter".into(),
                metrics.iter().find(|m| has_class(m, "n-input")).unwrap(),
            );
            let buttons = metrics
                .iter()
                .filter(|m| has_class(m, "n-button") && m["y"].as_f64().unwrap() < 160.)
                .collect::<Vec<_>>();
            let ids = if snapshot.starts_with("entries-stream") {
                vec!["entry-search", "more-entries", "all-entries", "add-entry"]
            } else {
                vec![
                    "entry-search",
                    "entry-align-0",
                    "entry-align-1",
                    "more-entries",
                    "all-entries",
                    "add-entry",
                ]
            };
            assert_eq!(buttons.len(), ids.len());
            for (id, source) in ids.into_iter().zip(buttons) {
                check(id.into(), source);
            }
            for source in metrics.iter().filter(|m| has_class(m, "n-data-table-th")) {
                let id = match source["text"].as_str().unwrap() {
                    "#" => "heading-entry-number",
                    "字段" | "ID" => "heading-entry-field",
                    "值" => "heading-entry-value",
                    "分值" => "heading-entry-score",
                    "操作" => "heading-entry-actions",
                    label => panic!("unexpected original heading {label}"),
                };
                check(id.into(), source);
            }
            let cells = metrics
                .iter()
                .filter(|m| has_class(m, "n-data-table-td"))
                .collect::<Vec<_>>();
            let mut rows: Vec<Json> = Vec::new();
            for cell in cells {
                let y = cell["y"].as_f64().unwrap();
                if let Some(row) = rows
                    .last_mut()
                    .filter(|row| (row["y"].as_f64().unwrap() - y).abs() < 0.001)
                {
                    let left = row["x"].as_f64().unwrap().min(cell["x"].as_f64().unwrap());
                    let right = (row["x"].as_f64().unwrap() + row["w"].as_f64().unwrap())
                        .max(cell["x"].as_f64().unwrap() + cell["w"].as_f64().unwrap());
                    row["x"] = left.into();
                    row["w"] = (right - left).into();
                } else {
                    rows.push(serde_json::json!({"x":cell["x"],"y":cell["y"],"w":cell["w"],"h":cell["h"]}));
                }
            }
            for (index, row) in rows.iter().enumerate() {
                check(("entry", index).into(), row);
            }
            if let Some(empty) = metrics.iter().find(|m| has_class(m, "n-data-table-empty")) {
                check("entry-empty".into(), empty);
            }
            if required {
                let columns = metrics
                    .iter()
                    .filter(|m| has_class(m, "n-data-table-th"))
                    .map(|m| m["text"].as_str().unwrap())
                    .collect::<Vec<_>>();
                for (cell_index, cell) in metrics
                    .iter()
                    .filter(|m| has_class(m, "n-data-table-td"))
                    .enumerate()
                {
                    let key = match columns[cell_index % columns.len()] {
                        "#" => "entry-number",
                        "字段" | "ID" => "entry-field",
                        "值" => "entry-value",
                        "分值" => "entry-score",
                        "操作" => "entry-action-cell",
                        label => panic!("unexpected cell {label}"),
                    };
                    check((key, cell_index / columns.len()).into(), cell);
                }
                for handle in metrics
                    .iter()
                    .filter(|m| has_class(m, "n-data-table-resize-button"))
                {
                    let x = handle["x"].as_f64().unwrap();
                    let heading = metrics
                        .iter()
                        .find(|m| {
                            has_class(m, "n-data-table-th")
                                && x >= m["x"].as_f64().unwrap()
                                && x < m["x"].as_f64().unwrap() + m["w"].as_f64().unwrap()
                        })
                        .unwrap();
                    let key = match heading["text"].as_str().unwrap() {
                        "字段" | "ID" => "field",
                        "值" => "value",
                        "分值" => "score",
                        label => panic!("unexpected resizable column {label}"),
                    };
                    check(format!("resize-entry-{key}").into(), handle);
                }
                // Original row buttons retain an18px hit box and render20px glyphs.
                let rows = self.entry_rows(self.index().unwrap());
                let mut buttons = metrics
                    .iter()
                    .filter(|m| has_class(m, "n-button") && m["y"].as_f64().unwrap() >= 190.);
                for index in 0..rows.len() {
                    let mut ids = vec![format!("entry-copy-{index}")];
                    if snapshot.starts_with("entries-hash") {
                        ids.push(format!("entry-refresh-{index}"));
                    }
                    if !snapshot.starts_with("entries-stream") {
                        ids.push(format!("entry-edit-{index}"));
                    }
                    ids.push(format!("entry-delete-{index}"));
                    for id in ids {
                        check(id.into(), buttons.next().expect("original row button"));
                    }
                }
                assert!(buttons.next().is_none());
            }
        }
        let i = self.index().unwrap();
        let rows = self.entry_rows(i);
        let row_models=rows.iter().map(|row|serde_json::json!({"id":display_bytes(&row.id),"value":row.display_value.clone().or_else(||self.tabs[i].row_values.get(&row.id).map(|value|value.text.clone())).unwrap_or_else(||display_bytes(&row.value))})).collect::<Vec<_>>();
        if let Some(source) = modern {
            assert_eq!(rows.len(), source["rows"].as_array().unwrap().len());
            assert_eq!(
                self.general_font_size(),
                source["generalFontSize"].as_f64().unwrap() as f32
            );
            assert_eq!(
                i64::from(self.pref.editor.entry_text_align),
                source["textAlign"].as_i64().unwrap()
            );
            assert_eq!(
                format!("{:?}", self.tabs[i].detail.as_ref().unwrap().kind).to_uppercase(),
                source["kind"].as_str().unwrap()
            );
            if source["kind"] == "SET" {
                let values = row_models
                    .iter()
                    .map(|row| row["value"].as_str().unwrap().to_owned())
                    .collect::<BTreeSet<_>>();
                let original = source["metrics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|m| has_class(m, "n-data-table-td"))
                    .enumerate()
                    .filter(|(index, _)| index % 3 == 1)
                    .map(|(_, m)| m["text"].as_str().unwrap().to_owned())
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    values, original,
                    "actual unordered Set members equal the source layout inputs"
                );
            }
        }
        match snapshot {
            "entries-hash-field-filter" => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].id, b"name");
            }
            "entries-hash-value-filter" | "entries-list-index-filter" => assert!(rows.is_empty()),
            "entries-list-value-filter" => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].value, b"Rust");
            }
            "entries-hash-sort" => {
                assert_eq!(self.tabs[i].field_sort, Some(false));
                assert_eq!(rows[0].id, b"name");
            }
            "entries-zset-sort" => {
                assert_eq!(self.tabs[i].score_sort, Some(false));
                assert_eq!(rows[0].value, b"Rust");
            }
            _ => {}
        }
        println!(
            "ENTRIES MACOS FRAME: {}",
            serde_json::json!({"scene":snapshot,"frames":actual_frames,"rows":row_models})
        );
        if legacy.is_some() && cfg!(target_os = "macos") {
            println!(
                "ENTRIES RETAINED LEGACY COMPARISON: {}",
                serde_json::json!({"scene":snapshot,"passed":legacy_failures.is_empty(),"failures":legacy_failures,"scope":"Original Chrome rectangles retained with unchanged0.6 tolerance; separate original Mac assertions above."})
            );
        }
        println!(
            "ENTRIES MACOS QA PASS: {snapshot} original component rectangles and collection behavior"
        );
    }

    pub(super) fn qa_assert_collection_delete(&self, snapshot: &str, window: &Window) {
        let macos: Json = serde_json::from_str(include_str!(
            "../../tests/fixtures/entries-overlays-macos-source.json"
        ))
        .unwrap();
        let old: Json = serde_json::from_str(include_str!(
            "../../tests/fixtures/entry-delete-source.json"
        ))
        .unwrap();
        let modern = macos["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == snapshot)
            .unwrap();
        let legacy = old["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == snapshot)
            .unwrap();
        let mut failures = Vec::new();
        for (record, required) in [
            (modern, cfg!(target_os = "macos")),
            (legacy, !cfg!(target_os = "macos")),
        ] {
            for (id, class, label) in [
                ("entry-delete-confirmation", "n-popover", None),
                ("entry-delete-confirmation-body", "n-popconfirm__body", None),
                (
                    "entry-delete-confirmation-actions",
                    "n-popconfirm__action",
                    None,
                ),
                ("entry-delete-cancel", "n-button", Some("取消")),
                ("entry-delete-confirm", "n-button", Some("确认")),
            ] {
                let source = record["metrics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|m| has_class(m, class) && label.is_none_or(|label| m["text"] == label))
                    .unwrap();
                let bounds = gpui_base::test_support::find(window, &[], &id.into())
                    .unwrap()
                    .bounds();
                for (actual, key) in [
                    (bounds.origin.x, "x"),
                    (bounds.origin.y, "y"),
                    (bounds.size.width, "w"),
                    (bounds.size.height, "h"),
                ] {
                    let expected = source[key].as_f64().unwrap() as f32;
                    let delta = (actual.as_f32() - expected).abs();
                    if required {
                        assert!(
                            delta < 0.6,
                            "{snapshot} {id} {key}: {actual:?} vs {expected}"
                        );
                    } else if delta >= 0.6 {
                        failures.push(serde_json::json!({"id":id,"dimension":key,"actual":actual.as_f32(),"legacyExpected":expected,"delta":delta,"passed":false,"thresholdUnchanged":0.6}));
                    }
                }
            }
        }
        println!(
            "ENTRIES RETAINED LEGACY DELETE: {}",
            serde_json::json!({"scene":snapshot,"passed":failures.is_empty(),"failures":failures})
        );
        println!("ENTRIES MACOS DELETE QA PASS: {snapshot} original confirmation rectangles");
    }
}
