//! Locale-aware integer tick labels. Chart.js uses Intl.NumberFormat with zero
//! fractional digits for these precision=0 axes, including scientific notation
//! when the full tick domain exceeds 1e15. Byte axes have their own callback.

pub(super) fn format(value: f64, scientific: bool) -> String {
    if value == 0. {
        return "0".into();
    }
    #[cfg(target_os = "macos")]
    if let Some(text) = macos::format(value, scientific) {
        return text;
    }
    portable(value, scientific)
}

// Other desktop platforms currently use the English numeric convention. Their
// native locale adapters still need comparison with the original WebView.
fn portable(value: f64, scientific: bool) -> String {
    if scientific && value.is_finite() {
        let mut exponent = value.abs().log10().floor() as i32;
        let mut mantissa = (value.abs() / 10f64.powi(exponent)).round();
        if mantissa >= 10. {
            mantissa /= 10.;
            exponent += 1;
        }
        return format!(
            "{}{mantissa:.0}E{exponent}",
            if value < 0. { "-" } else { "" }
        );
    }
    let digits = format!("{:.0}", value.abs().round());
    let mut text = if value < 0. {
        "-".into()
    } else {
        String::new()
    };
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{ffi::c_void, ptr};

    #[repr(C)]
    struct Range {
        location: isize,
        length: isize,
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyCurrent() -> *const c_void;
        fn CFNumberFormatterCreate(
            allocator: *const c_void,
            locale: *const c_void,
            style: isize,
        ) -> *const c_void;
        fn CFNumberCreate(
            allocator: *const c_void,
            kind: isize,
            value: *const c_void,
        ) -> *const c_void;
        fn CFNumberFormatterSetProperty(
            formatter: *const c_void,
            key: *const c_void,
            value: *const c_void,
        );
        fn CFNumberFormatterCreateStringWithValue(
            allocator: *const c_void,
            formatter: *const c_void,
            kind: isize,
            value: *const c_void,
        ) -> *const c_void;
        fn CFStringGetLength(text: *const c_void) -> isize;
        fn CFStringGetCharacters(text: *const c_void, range: Range, buffer: *mut u16);
        fn CFRelease(object: *const c_void);
        static kCFNumberFormatterMinFractionDigits: *const c_void;
        static kCFNumberFormatterMaxFractionDigits: *const c_void;
        static kCFNumberFormatterRoundingMode: *const c_void;
        static kCFNumberFormatterMinSignificantDigits: *const c_void;
        static kCFNumberFormatterMaxSignificantDigits: *const c_void;
    }

    struct Owned(*const c_void);
    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) };
            }
        }
    }
    fn formatter(scientific: bool) -> Owned {
        // CoreFoundation number formatters are not thread safe; each cache is
        // thread local. Every Copy/Create result has a matching release.
        unsafe {
            let locale = Owned(CFLocaleCopyCurrent());
            let result = Owned(CFNumberFormatterCreate(
                ptr::null(),
                locale.0,
                if scientific { 4 } else { 1 },
            ));
            if !result.0.is_null() {
                for (key, value) in [
                    (kCFNumberFormatterMinFractionDigits, 0i32),
                    (kCFNumberFormatterMaxFractionDigits, 0i32),
                    // Intl's default halfExpand, rather than CF's halfEven.
                    (kCFNumberFormatterRoundingMode, 6i32),
                ] {
                    let number = Owned(CFNumberCreate(
                        ptr::null(),
                        3,
                        (&value as *const i32).cast(),
                    ));
                    if !number.0.is_null() {
                        CFNumberFormatterSetProperty(result.0, key, number.0);
                    }
                }
                if scientific {
                    // CF's scientific style enables significant digits, which
                    // otherwise overrides maximumFractionDigits. Intl with
                    // zero fractional digits rounds the mantissa to one digit.
                    let one = 1i32;
                    let number = Owned(CFNumberCreate(ptr::null(), 3, (&one as *const i32).cast()));
                    if !number.0.is_null() {
                        CFNumberFormatterSetProperty(
                            result.0,
                            kCFNumberFormatterMinSignificantDigits,
                            number.0,
                        );
                        CFNumberFormatterSetProperty(
                            result.0,
                            kCFNumberFormatterMaxSignificantDigits,
                            number.0,
                        );
                    }
                }
            }
            result
        }
    }
    thread_local! {
        static DECIMAL: Owned = formatter(false);
        static SCIENTIFIC: Owned = formatter(true);
    }
    pub(super) fn format(value: f64, scientific: bool) -> Option<String> {
        let cache = if scientific { &SCIENTIFIC } else { &DECIMAL };
        cache.with(|formatter| unsafe {
            if formatter.0.is_null() {
                return None;
            }
            let text = Owned(CFNumberFormatterCreateStringWithValue(
                ptr::null(),
                formatter.0,
                13,
                (&value as *const f64).cast(),
            ));
            if text.0.is_null() {
                return None;
            }
            let length = CFStringGetLength(text.0);
            if !(0..=4096).contains(&length) {
                return None;
            }
            let mut units = vec![0; length as usize];
            CFStringGetCharacters(
                text.0,
                Range {
                    location: 0,
                    length,
                },
                units.as_mut_ptr(),
            );
            String::from_utf16(&units).ok()
        })
    }
}
