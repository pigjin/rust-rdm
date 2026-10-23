use super::Assets;
use gpui::WindowAppearance;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::{NSBundle, NSData, NSProcessInfo, NSString, ns_string};
use std::{cell::Cell, sync::OnceLock};

fn fallback_icon(native_icon: bool, appearance: WindowAppearance) -> Option<&'static str> {
    if native_icon {
        None
    } else if matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    ) {
        Some("images/icon-dark.png")
    } else {
        Some("images/icon-light.png")
    }
}

pub(super) fn sync(appearance: WindowAppearance) {
    static NATIVE_ICON: OnceLock<bool> = OnceLock::new();
    thread_local! { static LAST_ICON: Cell<Option<&'static str>> = const { Cell::new(None) }; }
    let native_icon = *NATIVE_ICON.get_or_init(|| {
        let bundle = NSBundle::mainBundle();
        NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
            >= 26
            && bundle
                .objectForInfoDictionaryKey(ns_string!("CFBundleIconName"))
                .and_then(|name| name.downcast::<NSString>().ok())
                .is_some_and(|name| &*name == ns_string!("AppIcon"))
            && bundle
                .pathForResource_ofType(Some(ns_string!("Assets")), Some(ns_string!("car")))
                .is_some()
    });
    // A runtime NSImage overrides the system's native dark, clear and tinted icon variants.
    let Some(asset) = fallback_icon(native_icon, appearance) else {
        return;
    };
    if LAST_ICON.get() == Some(asset) {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(file) = Assets::get(asset) else {
        eprintln!("Missing Dock icon: {asset}");
        return;
    };
    let data = NSData::with_bytes(file.data.as_ref());
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("Invalid Dock icon: {asset}");
        return;
    };
    unsafe {
        NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image));
    }
    LAST_ICON.set(Some(asset));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_variants_are_never_replaced_and_fallback_uses_system_appearance() {
        for appearance in [
            WindowAppearance::Light,
            WindowAppearance::VibrantLight,
            WindowAppearance::Dark,
            WindowAppearance::VibrantDark,
        ] {
            assert_eq!(fallback_icon(true, appearance), None);
            let asset = fallback_icon(false, appearance).unwrap();
            assert!(Assets::get(asset).is_some());
            assert_eq!(
                asset.ends_with("dark.png"),
                matches!(
                    appearance,
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                )
            );
        }
    }
}
