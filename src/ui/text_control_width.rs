//! Intrinsic text-control sizing for the original macOS WebKit layout.
//!
//! Uses public native font tables rather than a width fitted to one screenshot.
//! Reference: WebKit FontCascade, FontCoreText and RenderTextControlSingleLine.
//! Independent original-control measurements are in input-metrics-macos-source.json.

#[cfg(target_os = "macos")]
mod macos {
    use objc2_app_kit::NSFont;
    use objc2_foundation::NSString;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct Advance {
        width: f64,
        height: f64,
    }

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontGetUnitsPerEm(font: *const c_void) -> u32;
        fn CTFontCopyTable(font: *const c_void, tag: u32, options: u32) -> *const c_void;
        fn CTFontGetGlyphsForCharacters(
            font: *const c_void,
            text: *const u16,
            glyph: *mut u16,
            count: isize,
        ) -> bool;
        fn CTFontGetAdvancesForGlyphs(
            font: *const c_void,
            orientation: u32,
            glyph: *const u16,
            advance: *mut Advance,
            count: isize,
        ) -> f64;
        fn CTFontGetXHeight(font: *const c_void) -> f64;
        fn CTFontGetAscent(font: *const c_void) -> f64;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataGetLength(data: *const c_void) -> isize;
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
        fn CFRelease(object: *const c_void);
    }

    fn table(font: *const c_void, tag: u32) -> Option<Vec<u8>> {
        // NSFont and CTFont are toll-free bridged. The NSFont owner remains alive
        // for every call; CopyTable returns one CF object that we release here.
        unsafe {
            let data = CTFontCopyTable(font, tag, 0);
            if data.is_null() {
                return None;
            }
            let length = CFDataGetLength(data);
            let bytes = CFDataGetBytePtr(data);
            let result = if length > 0 && !bytes.is_null() {
                Some(std::slice::from_raw_parts(bytes, length as usize).to_vec())
            } else {
                None
            };
            CFRelease(data);
            result
        }
    }
    fn signed(bytes: &[u8], offset: usize) -> Option<i16> {
        Some(i16::from_be_bytes(
            bytes.get(offset..offset + 2)?.try_into().ok()?,
        ))
    }

    // These families have unreliable OS/2 averages in WebKit's native sizing.
    // Font names and the metric rule follow the public WebKit source; this is
    // data, not a replacement for GPUI's glyph shaping or font selection.
    fn uses_zero_advance(family: &str, system: bool) -> bool {
        system
            || [
                "#GungSeo",
                "#HeadLineA",
                "#PCMyungjo",
                "#PilGi",
                "American Typewriter",
                "Apple Braille",
                "Apple LiGothic",
                "Apple LiSung",
                "Apple Symbols",
                "AppleGothic",
                "AppleMyungjo",
                "Arial Hebrew",
                "Chalkboard",
                "Cochin",
                "Corsiva Hebrew",
                "Courier",
                "Euphemia UCAS",
                "Geneva",
                "Gill Sans",
                "Hei",
                "Helvetica",
                "Hoefler Text",
                "InaiMathi",
                "Kai",
                "Lucida Grande",
                "Marker Felt",
                "Monaco",
                "Mshtakan",
                "New Peninim MT",
                "Osaka",
                "Raanana",
                "STHeiti",
                "Symbol",
                "Times",
            ]
            .contains(&family)
    }

    pub(super) fn available_family(family: &str) -> Option<String> {
        if family.is_empty() { return None; }
        // Native family inventories omit some usable legacy fonts, including
        // Courier and Times. Ask the same native lookup used by control sizing.
        let font = NSFont::fontWithName_size(&NSString::from_str(family), 12.)?;
        let name = font.familyName()?.to_string();
        name.eq_ignore_ascii_case(family).then_some(name)
    }

    pub(super) fn intrinsic(families: &[String], point: f32, columns: usize) -> f32 {
        let point = f64::from(point);
        let selected = families.iter().find_map(|family| {
            if matches!(
                family.as_str(),
                ".SystemUIFont" | "system-ui" | "-apple-system" | "BlinkMacSystemFont"
            ) {
                Some((NSFont::systemFontOfSize(point), family.as_str(), true))
            } else {
                NSFont::fontWithName_size(&NSString::from_str(family), point)
                    .map(|font| (font, family.as_str(), false))
            }
        });
        let (font, family, system) =
            selected.unwrap_or_else(|| (NSFont::systemFontOfSize(point), ".SystemUIFont", true));
        let native = (&*font as *const NSFont).cast::<c_void>();
        let units = unsafe { CTFontGetUnitsPerEm(native) }.max(1) as f64;
        let mut glyph = 0;
        let mut zero = Advance::default();
        unsafe {
            if CTFontGetGlyphsForCharacters(native, &48, &mut glyph, 1) {
                CTFontGetAdvancesForGlyphs(native, 1, &glyph, &mut zero, 1);
            }
        }
        let fallback = if zero.width > 0. {
            zero.width
        } else {
            unsafe { CTFontGetXHeight(native) }
        };
        let average = table(native, u32::from_be_bytes(*b"OS/2"))
            .and_then(|bytes| signed(&bytes, 2))
            .map(|value| f64::from(value) / units * point)
            .filter(|value| *value > 0.)
            .unwrap_or(fallback);
        let maximum = table(native, u32::from_be_bytes(*b"head"))
            .and_then(|bytes| {
                Some(
                    f64::from(i32::from(signed(&bytes, 40)?) - i32::from(signed(&bytes, 36)?))
                        / units
                        * point,
                )
            })
            .filter(|value| *value > 0.)
            .unwrap_or_else(|| average.max(unsafe { CTFontGetAscent(native) }));
        let use_average = !uses_zero_advance(family, system);
        let lucida = family == "Lucida Grande";
        let character = if lucida {
            (point * 901. / 2048.).round()
        } else if use_average {
            average.round()
        } else {
            fallback
        };
        let base = (character * columns as f64 * 64.).ceil() / 64.;
        let maximum = if lucida {
            (point * 4027. / 2048.).round()
        } else {
            maximum.round()
        };
        let extra = if use_average || lucida {
            ((maximum - character) * 64.).trunc() / 64.
        } else {
            0.
        };
        // The independent zero-border/padding WK controls retain 2px of end space
        // at every captured size, column count and family (120 measurements).
        (base + extra + 2.) as f32
    }
}

#[cfg(target_os = "macos")]
pub(super) fn intrinsic(families: &[String], point: f32, columns: usize) -> f32 {
    macos::intrinsic(families, point, columns)
}

#[cfg(target_os = "macos")]
pub(super) fn available_family(family: &str) -> Option<String> {
    macos::available_family(family)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn original_resolvable_legacy_font_families() {
        let source: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/css-label-macos-source.json"
        )).unwrap();
        for row in source["records"].as_array().unwrap() {
            let family = row["configuration"]["family"].as_str().unwrap();
            if family.is_empty() { continue; }
            for name in [family.to_owned(), family.to_ascii_lowercase(), family.to_ascii_uppercase()] {
                assert_eq!(super::available_family(&name).as_deref(), Some(family), "{name}");
            }
        }
        assert!(super::available_family("RDM Missing Font QA 20261020").is_none());
        assert!(super::available_family("").is_none());
    }

    #[test]
    fn original_webkit_intrinsic_control_widths() {
        for source in [
            include_str!("../../tests/fixtures/input-metrics-macos-source.json"),
            include_str!("../../tests/fixtures/input-metrics-macos-extra-source.json"),
        ] {
            let reference: serde_json::Value = serde_json::from_str(source).unwrap();
            for row in reference["samples"].as_array().unwrap() {
                let family = row["family"].as_str().unwrap().to_owned();
                let actual = super::intrinsic(
                    &[family],
                    row["point"].as_f64().unwrap() as f32,
                    row["count"].as_u64().unwrap() as usize,
                );
                assert!(
                    (actual - row["width"].as_f64().unwrap() as f32).abs() < 0.02,
                    "{row}: {actual}"
                );
            }
        }
    }
}
