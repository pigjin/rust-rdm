use gpui::{App, Global};
use objc2::{MainThreadMarker, rc::Retained};
#[cfg(feature = "ui-qa")]
use objc2_app_kit::NSMenu;
use objc2_app_kit::{NSApplication, NSMenuItem};
use objc2_foundation::NSString;
use serde_json::Value;

// AppKit can rename Preferences during insertion; capture by menu index and GPUI action tag.
const LABELS: &[(isize, Option<isize>, &str)] = &[
    (1, None, "edit"),
    (2, None, "window"),
    (0, Some(0), "preferences"),
    (0, Some(1), "quit"),
    (1, Some(2), "undo"),
    (1, Some(3), "redo"),
    (1, Some(4), "cut"),
    (1, Some(5), "copy"),
    (1, Some(6), "paste"),
    (1, Some(7), "select_all"),
    (2, Some(8), "close_tab"),
];

struct MenuLabels(Vec<(&'static str, Retained<NSMenuItem>)>);
impl Global for MenuLabels {}

#[cfg(feature = "ui-qa")]
fn item(menu: &NSMenu, title: &str) -> Retained<NSMenuItem> {
    let index = menu.indexOfItemWithTitle(&NSString::from_str(title));
    assert!(index >= 0, "missing application menu item: {title}");
    menu.itemAtIndex(index)
        .unwrap_or_else(|| panic!("missing application menu item: {title}"))
}

pub(super) fn init(cx: &mut App) {
    let mtm = MainThreadMarker::new().expect("application menus run on the main thread");
    let main = NSApplication::sharedApplication(mtm)
        .mainMenu()
        .expect("GPUI initialized the application menu");
    let labels = LABELS
        .iter()
        .map(|&(index, tag, key)| {
            let menu = main.itemAtIndex(index).expect("application menu");
            let item = if let Some(tag) = tag {
                menu.submenu()
                    .expect("application submenu")
                    .itemWithTag(tag)
                    .expect("GPUI application menu action")
            } else {
                menu
            };
            (key, item)
        })
        .collect();
    cx.set_global(MenuLabels(labels));
}

fn title(lang: &Value, key: &str) -> String {
    let text = lang["menu"][key].as_str().unwrap_or(key);
    if key == "preferences" {
        format!("{text}…")
    } else {
        text.to_owned()
    }
}

pub(super) fn sync(lang: &Value, cx: &App) {
    // GPUI recognizes "Window" at creation; retain its native registration and action bindings.
    for (key, item) in &cx.global::<MenuLabels>().0 {
        let title = NSString::from_str(&title(lang, key));
        item.setTitle(&title);
        if let Some(submenu) = item.submenu() {
            submenu.setTitle(&title);
        }
    }
}

#[cfg(feature = "ui-qa")]
fn verify(lang: &Value, cx: &App) {
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    let main = app.mainMenu().unwrap();
    assert_eq!(item(&main, "RedisDock").title().to_string(), "RedisDock");
    for (key, original) in &cx.global::<MenuLabels>().0 {
        let expected = title(lang, key);
        assert_eq!(original.title().to_string(), expected, "native menu {key}");
        // The application's retained main menu owns every parent submenu.
        let parent = unsafe { original.menu() }.expect("menu item remains attached");
        assert!(std::ptr::eq(&*item(&parent, &expected), &**original));
        if let Some(submenu) = original.submenu() {
            assert!(std::ptr::eq(&*parent, &*main));
            assert_eq!(submenu.title().to_string(), expected);
            if *key == "window" {
                assert!(
                    std::ptr::eq(&*app.windowsMenu().unwrap(), &*submenu),
                    "localized Window retains the native windows menu registration"
                );
            }
        } else {
            assert!(main.indexOfItemWithSubmenu(Some(&parent)) >= 0);
        }
    }
}

#[cfg(feature = "ui-qa")]
pub(super) async fn verify_flow(
    view: &gpui::Entity<super::RdmApp>,
    window: gpui::WindowHandle<gpui_component::Root>,
    cx: &mut gpui::AsyncApp,
) {
    use super::*;
    let (original, saved, original_lang, bindings) = window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(w, cx);
                verify(&app.lang, cx);
                let bindings = cx
                    .global::<MenuLabels>()
                    .0
                    .iter()
                    .map(|(_, item)| {
                        (
                            item.action(),
                            item.tag(),
                            item.keyEquivalent().to_string(),
                            item.keyEquivalentModifierMask(),
                        )
                    })
                    .collect::<Vec<_>>();
                let state = (
                    app.pref.clone(),
                    app.store.preferences().unwrap(),
                    app.lang.clone(),
                    bindings,
                );
                app.open_dialog(dialogs::DialogKind::Preferences, w, cx);
                state
            })
        })
        .unwrap();
    for (label, expected) in [("简体中文", "编辑"), ("English", "Edit")] {
        cx.background_executor()
            .timer(Duration::from_millis(180))
            .await;
        qa_json::click("choice-general.language", window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(80))
            .await;
        cx.update_window(window.into(), |_, w, cx| {
            w.refresh();
            let _ = w.draw(cx);
            let position = gpui_base::test_support::snapshots(w)
                .into_iter()
                .find(|item| item.role() == Some(Role::MenuItem) && item.label() == Some(label))
                .unwrap_or_else(|| panic!("missing language option: {label}"))
                .bounds()
                .center();
            w.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
            w.refresh();
            let _ = w.draw(cx);
            w.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            w.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    position,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(40))
            .await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert_eq!(
                    app.tr("menu.edit"),
                    expected,
                    "actual language dropdown selection"
                );
                verify(&app.lang, cx);
            })
            .unwrap();
    }
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                for (code, file) in [
                    ("en", "en-us"),
                    ("zh", "zh-cn"),
                    ("tw", "zh-tw"),
                    ("ja", "ja-jp"),
                    ("ko", "ko-kr"),
                    ("fr", "fr-fr"),
                    ("es", "es-es"),
                    ("pt", "pt-br"),
                    ("ru", "ru-ru"),
                    ("tr", "tr-tr"),
                    ("zh", "zh-cn"),
                    ("en", "en-us"),
                ] {
                    app.set_dialog_field("general.language", code, w, cx);
                    let expected: Json = serde_json::from_slice(
                        &Assets::get(&format!("langs/{file}.json")).unwrap().data,
                    )
                    .unwrap();
                    assert_eq!(app.lang, expected, "language preview {code}");
                    assert_eq!(app.pref.general.language, original.general.language);
                    verify(&app.lang, cx);
                }
                assert_eq!(
                    app.store.preferences().unwrap().general.language,
                    saved.general.language,
                    "previews must not persist the language"
                );
                app.close_dialog(w, cx);
                assert_eq!(
                    app.lang, original_lang,
                    "cancel restores the original language"
                );
                verify(&app.lang, cx);
                app.open_dialog(dialogs::DialogKind::Preferences, w, cx);
                app.set_dialog_field("general.language", "en", w, cx);
                app.submit_dialog(w, cx);
                assert!(app.dialog.is_none(), "saving closes preferences");
                assert_eq!(app.store.preferences().unwrap().general.language, "en");
                app.pref = app.store.preferences().unwrap();
                app.load_language(cx);
                assert_eq!(
                    app.tr("menu.edit"),
                    "Edit",
                    "persisted language is loaded on startup"
                );
                verify(&app.lang, cx);
                app.open_dialog(dialogs::DialogKind::Preferences, w, cx);
            })
        })
        .unwrap();
    qa_json::click("restore-preferences", window, cx);
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                assert_eq!(app.pref.general.language, "auto");
                assert_eq!(app.store.preferences().unwrap().general.language, "auto");
                let system = system_language();
                if let Some(asset) = Assets::get(&format!("langs/{system}.json")) {
                    let expected: Json = serde_json::from_slice(&asset.data).unwrap();
                    assert_eq!(
                        app.lang, expected,
                        "restore defaults follows the system language"
                    );
                }
                verify(&app.lang, cx);
                app.close_dialog(w, cx);
                app.pref.general.language = "unknown-language".into();
                app.load_language(cx);
                assert_eq!(
                    app.tr("menu.edit"),
                    "Edit",
                    "unknown languages fall back to English"
                );
                verify(&app.lang, cx);
                let current = cx
                    .global::<MenuLabels>()
                    .0
                    .iter()
                    .map(|(_, item)| {
                        (
                            item.action(),
                            item.tag(),
                            item.keyEquivalent().to_string(),
                            item.keyEquivalentModifierMask(),
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    current, bindings,
                    "language changes preserve native actions and shortcuts"
                );
                app.pref = original;
                app.store.save("preferences.yaml", &saved).unwrap();
                app.apply_theme(w, cx);
                app.load_language(cx);
                verify(&app.lang, cx);
                app.open_dialog(dialogs::DialogKind::Preferences, w, cx);
                app.set_dialog_field("general.language", "en", w, cx);
            })
        })
        .unwrap();
    println!(
        "NATIVE MENU LANGUAGE QA PASS: ten languages, preview, cancel, save/reload, defaults/system, English fallback; native Window registration, menu identities, actions and shortcuts retained"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Assets;

    #[test]
    fn all_menu_translations_are_complete() {
        let languages = Assets::iter()
            .filter(|path| path.starts_with("langs/"))
            .collect::<Vec<_>>();
        assert_eq!(languages.len(), 10);
        for path in languages {
            let lang: Value = serde_json::from_slice(&Assets::get(&path).unwrap().data).unwrap();
            for &(_, _, key) in LABELS {
                assert!(
                    !lang["menu"][key].as_str().unwrap_or("").trim().is_empty(),
                    "{path}: {key}"
                );
            }
            assert_eq!(
                title(&lang, "preferences"),
                format!("{}…", lang["menu"]["preferences"].as_str().unwrap())
            );
            assert!(title(&lang, "quit").contains("RedisDock"));
        }
    }
}
