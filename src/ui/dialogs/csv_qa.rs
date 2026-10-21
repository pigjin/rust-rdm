//! Layout oracle from the original Vue forms; transfer checks use disposable Redis.
use super::*;

fn source(scene: &str) -> Json {
    let fixture: Json = serde_json::from_str(if scene.starts_with("csv-dark-") {
        include_str!("../../../tests/fixtures/csv-dark-source.json")
    } else {
        include_str!("../../../tests/fixtures/csv-dialog-source.json")
    })
    .unwrap();
    fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["name"] == scene)
        .unwrap()
        .clone()
}
impl RdmApp {
    pub(in crate::ui) fn qa_prepare_csv(
        &mut self,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let record = source(scene);
        let dark = scene.starts_with("csv-dark-");
        let import = scene.starts_with("csv-import") || (dark && !scene.contains("export"));
        if dark {
            self.pref.general.theme = "dark".into();
            self.apply_theme(window, cx);
        }
        self.csv_import_unit = if scene.ends_with("reopened") { 60 } else { 1 };
        self.csv_export_expire = false;
        self.open_dialog(
            if import {
                DialogKind::Import
            } else {
                DialogKind::Export
            },
            window,
            cx,
        );
        self.set_dialog_field("server", "localhost", window, cx);
        self.set_dialog_field("db", "0", window, cx);
        self.set_dialog_field(
            "path",
            record["inputs"][2]["value"].as_str().unwrap(),
            window,
            cx,
        );
        let keys = if import || scene.ends_with("export-empty") {
            vec![]
        } else if scene == "csv-export-reopened" {
            vec![b"aastring".to_vec()]
        } else {
            vec![b"aastring".to_vec(), b"aahash".to_vec()]
        };
        self.dialog.as_mut().unwrap().target = Some(Target {
            id: 0,
            conn: Connection {
                name: "localhost".into(),
                ..Default::default()
            },
            db: 0,
            key: vec![],
            kind: KeyType::String,
            keys,
        });
        if import {
            let radios = record["radios"].as_array().unwrap();
            let conflict = radios[..2]
                .iter()
                .position(|item| item["checked"] == true)
                .unwrap();
            let ttl_type = radios[2..]
                .iter()
                .position(|item| item["checked"] == true)
                .unwrap();
            self.set_dialog_field("conflict", conflict.to_string(), window, cx);
            self.set_dialog_field("ttl_type", ttl_type.to_string(), window, cx);
            self.set_dialog_field(
                "reload",
                record["checkboxes"][0]["checked"]
                    .as_bool()
                    .unwrap()
                    .to_string(),
                window,
                cx,
            );
            if ttl_type == 2 {
                let value = record["inputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|field| field["readonly"] == false && field["placeholder"] == "请输入")
                    .unwrap()["value"]
                    .as_str()
                    .unwrap();
                self.set_dialog_field("ttl", value, window, cx);
                self.set_dialog_field(
                    "unit",
                    if record["selects"][0] == "分" {
                        "60"
                    } else {
                        "1"
                    },
                    window,
                    cx,
                );
            }
        } else {
            self.set_dialog_field(
                "include_ttl",
                record["checkboxes"][0]["checked"]
                    .as_bool()
                    .unwrap()
                    .to_string(),
                window,
                cx,
            );
        }
        if matches!(scene, "csv-dark-unit" | "csv-dark-unit-hover") {
            self.open_csv_unit(window, cx);
        } else if dark && matches!(scene, "csv-dark-custom" | "csv-dark-export-ready") {
            self.focus.focus(window, cx);
        } else {
            self.dialog.as_ref().unwrap().fields["server"]
                .update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    pub(in crate::ui) fn qa_assert_csv_layout(&self, scene: &str, window: &mut Window, cx: &App) {
        let record = source(scene);
        let rect = |id: &str| {
            gpui_base::test_support::find(
                window,
                &[],
                &ElementId::from(SharedString::from(id.to_string())),
            )
            .unwrap()
            .bounds()
        };
        let assert_rect = |id: &str, expected: &Json| {
            let actual = rect(id);
            for (name, actual, epsilon) in [
                ("x", actual.origin.x.as_f32(), 1.1),
                ("y", actual.origin.y.as_f32(), 1.1),
                ("width", actual.size.width.as_f32(), 0.6),
                ("height", actual.size.height.as_f32(), 0.6),
            ] {
                let expected = expected[name].as_f64().unwrap() as f32;
                assert!(
                    (actual - expected).abs() < epsilon,
                    "{scene} {id} {name}: {actual} vs source {expected}"
                );
            }
        };
        eprintln!(
            "CSV GEOMETRY {scene}: card {:?}; server {:?}; path {:?}; actions {:?}",
            rect("dialog-card"),
            rect("csv-server"),
            rect("csv-path-input"),
            rect("csv-actions")
        );
        let elements = record["elements"].as_array().unwrap();
        assert_rect(
            "dialog-card",
            record.get("bounds").unwrap_or_else(|| {
                &elements.iter().find(|e| has_class(e, "n-dialog")).unwrap()["bounds"]
            }),
        );
        let inputs = record["elements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|element| {
                element["classes"]
                    .as_str()
                    .unwrap()
                    .split_whitespace()
                    .any(|class| class == "n-input")
            })
            .collect::<Vec<_>>();
        for (index, id) in ["csv-server", "csv-db", "csv-path-input", "csv-ttl-input"]
            .into_iter()
            .enumerate()
        {
            if let Some(expected) = inputs.get(index) {
                assert_rect(id, &expected["bounds"]);
            }
        }
        let actions = record["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|element| element["classes"] == "n-dialog__action")
            .unwrap();
        assert_rect("csv-actions", &actions["bounds"]);
        if scene.starts_with("csv-dark-") {
            let unit = elements.iter().find(|e| has_class(e, "n-base-selection"));
            if let Some(unit) = unit {
                assert_rect("csv-unit", &unit["bounds"]);
            }
            if let Some(menu) = elements.iter().find(|e| has_class(e, "n-base-select-menu")) {
                assert_rect("csv-unit-options", &menu["bounds"]);
                let rows = elements
                    .iter()
                    .filter(|e| has_class(e, "n-base-select-option"))
                    .collect::<Vec<_>>();
                for (index, row) in rows.iter().enumerate() {
                    let id = ElementId::from(("csv-unit-option", index));
                    let actual = gpui_base::test_support::find(window, &[], &id)
                        .unwrap()
                        .bounds();
                    for (name, value) in [
                        ("x", actual.origin.x),
                        ("y", actual.origin.y),
                        ("width", actual.size.width),
                        ("height", actual.size.height),
                    ] {
                        assert!(
                            (value.as_f32() - row["bounds"][name].as_f64().unwrap() as f32).abs()
                                < 0.7,
                            "{scene} option {index} {name}"
                        );
                    }
                }
                assert_eq!(
                    self.dialog.as_ref().unwrap().csv_unit_picker.pending,
                    rows.iter()
                        .position(|e| e["classes"].as_str().unwrap().contains("--pending"))
                        .unwrap()
                );
            }
            if let Some(card) = elements.iter().find(|e| has_class(e, "n-card")) {
                assert_rect("csv-affected-keys", &card["bounds"]);
            }
            let dialog = self.dialog.as_ref().unwrap();
            assert_eq!(
                dialog.value("path", cx),
                record["inputs"][2]["value"].as_str().unwrap()
            );
            assert_eq!(
                dialog.boolean(if import_scene(scene) {
                    "reload"
                } else {
                    "include_ttl"
                }),
                record["checkboxes"][0]["checked"].as_bool().unwrap()
            );
        }
        let disabled = record["buttons"].as_array().unwrap().last().unwrap()["disabled"]
            .as_bool()
            .unwrap();
        assert_eq!(
            self.csv_confirm_enabled(cx),
            !disabled,
            "{scene} original confirmation availability"
        );
        eprintln!("QA PASS: {scene} source form, inputs and footer geometry");
    }
}

fn has_class(element: &Json, class: &str) -> bool {
    element["classes"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .any(|value| value == class)
}
fn import_scene(scene: &str) -> bool {
    !scene.contains("export")
}
