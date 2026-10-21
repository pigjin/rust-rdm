//! Rotated ASCII category labels. CoreText supplies the same font outlines and
//! advances used for the unrotated Helvetica Neue chart labels on macOS.
use super::*;

#[cfg(target_os = "macos")]
pub(super) fn paint_rotated(
    text: &str,
    origin: Point<Pixels>,
    angle: f64,
    color: Hsla,
    window: &mut Window,
    _: &mut App,
) {
    use objc2_app_kit::NSFont;
    use objc2_foundation::NSString;
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct NativePoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Advance {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    struct PathElement {
        kind: i32,
        points: *const NativePoint,
    }
    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontGetGlyphsForCharacters(
            font: *const c_void,
            text: *const u16,
            glyphs: *mut u16,
            count: isize,
        ) -> bool;
        fn CTFontGetAdvancesForGlyphs(
            font: *const c_void,
            orientation: u32,
            glyphs: *const u16,
            advances: *mut Advance,
            count: isize,
        ) -> f64;
        fn CTFontCreatePathForGlyph(
            font: *const c_void,
            glyph: u16,
            transform: *const c_void,
        ) -> *const c_void;
        fn CTFontGetAscent(font: *const c_void) -> f64;
        fn CTFontGetDescent(font: *const c_void) -> f64;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPathApply(
            path: *const c_void,
            info: *mut c_void,
            callback: unsafe extern "C" fn(*mut c_void, *const PathElement),
        );
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: *const c_void);
    }
    struct Builder {
        path: PathBuilder,
        origin: Point<Pixels>,
        angle: f64,
        x: f64,
        baseline: f64,
    }
    unsafe extern "C" fn append(info: *mut c_void, element: *const PathElement) {
        // CoreGraphics calls synchronously; pointers remain valid for this call.
        let b = unsafe { &mut *info.cast::<Builder>() };
        let e = unsafe { &*element };
        let p = |i: usize| {
            let q = unsafe { *e.points.add(i) };
            let x = b.x + q.x;
            let y = b.baseline - q.y;
            b.origin
                + point(
                    px((x * b.angle.cos() - y * b.angle.sin()) as f32),
                    px((x * b.angle.sin() + y * b.angle.cos()) as f32),
                )
        };
        match e.kind {
            0 => b.path.move_to(p(0)),
            1 => b.path.line_to(p(0)),
            2 => b.path.curve_to(p(1), p(0)),
            3 => b.path.cubic_bezier_to(p(2), p(0), p(1)),
            4 => b.path.close(),
            _ => {}
        }
    }
    let Some(font) = NSFont::fontWithName_size(&NSString::from_str("Helvetica Neue"), 12.) else {
        return;
    };
    let native = (&*font as *const NSFont).cast::<c_void>();
    let chars: Vec<u16> = text.encode_utf16().collect();
    let mut glyphs = vec![0; chars.len()];
    let mut advances = vec![Advance::default(); chars.len()];
    unsafe {
        if !CTFontGetGlyphsForCharacters(
            native,
            chars.as_ptr(),
            glyphs.as_mut_ptr(),
            chars.len() as isize,
        ) {
            return;
        }
        let width = CTFontGetAdvancesForGlyphs(
            native,
            1,
            glyphs.as_ptr(),
            advances.as_mut_ptr(),
            chars.len() as isize,
        );
        let mut b = Builder {
            path: PathBuilder::fill(),
            origin,
            angle,
            x: -width,
            baseline: 7.2 + (CTFontGetAscent(native) - CTFontGetDescent(native)) / 2.,
        };
        for (glyph, advance) in glyphs.into_iter().zip(advances) {
            let path = CTFontCreatePathForGlyph(native, glyph, std::ptr::null());
            if !path.is_null() {
                CGPathApply(path, (&mut b as *mut Builder).cast(), append);
                CFRelease(path);
            }
            b.x += advance.width;
        }
        if let Ok(path) = b.path.build() {
            window.paint_path(path, color);
        }
    }
}
#[cfg(not(target_os = "macos"))]
pub(super) fn paint_rotated(
    text: &str,
    origin: Point<Pixels>,
    _: f64,
    color: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let line = window.text_system().shape_line(
        text.to_owned().into(),
        px(12.),
        &[TextRun {
            len: text.len(),
            font: gpui::font("Helvetica Neue"),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let _ = line.paint(
        origin - point(line.width, px(0.)),
        px(14.4),
        TextAlign::Left,
        None,
        window,
        cx,
    );
}
