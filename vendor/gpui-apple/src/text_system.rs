use anyhow::anyhow;
use collections::{HashMap, HashSet};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    attributed_string::CFMutableAttributedString,
    base::{CFRange, TCFType},
    number::CFNumber,
    string::CFString,
};
use core_graphics::{
    base::{CGFloat, CGGlyph, kCGImageAlphaPremultipliedLast},
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    geometry::CGPoint,
};
use core_text::{
    font::CTFont,
    font_collection::CTFontCollectionRef,
    font_descriptor::{
        CTFontDescriptor, kCTFontSlantTrait, kCTFontSymbolicTrait, kCTFontWeightTrait,
        kCTFontWidthTrait,
    },
    line::CTLine,
    string_attributes::kCTFontAttributeName,
};
use font_kit::{
    font::Font as FontKitFont,
    handle::Handle,
    hinting::HintingOptions,
    metrics::Metrics,
    properties::{Style as FontkitStyle, Weight as FontkitWeight},
    source::SystemSource,
    sources::mem::MemSource,
};
use gpui::{
    Bounds, DevicePixels, Font, FontFallbacks, FontFeatures, FontId, FontMetrics, FontRun,
    FontStyle, FontWeight, GlyphId, LineLayout, Pixels, PlatformTextSystem, RenderGlyphParams,
    Result, SUBPIXEL_VARIANTS_X, ShapedGlyph, ShapedRun, SharedString, Size, TextRenderingMode,
    point, px, size, swap_rgba_pa_to_bgra,
};
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use pathfinder_geometry::{
    rect::{RectF, RectI},
    transform2d::Transform2F,
    vector::Vector2F,
};
use smallvec::SmallVec;
use std::{borrow::Cow, char, convert::TryFrom, sync::Arc};

#[cfg(target_os = "macos")]
use {
    core_foundation::base::CFType,
    gpui::{Hsla, Rgba},
    std::sync::OnceLock,
};

use crate::open_type::apply_features_and_fallbacks;

#[allow(non_upper_case_globals)]
const kCGImageAlphaOnly: u32 = 7;

/// Apple text system using CoreText for font shaping.
pub struct AppleTextSystem(RwLock<AppleTextSystemState>);

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontKey {
    font_family: SharedString,
    font_features: FontFeatures,
    font_fallbacks: Option<FontFallbacks>,
}

#[derive(Clone, PartialEq, Eq)]
struct NativeFontKey(CTFont);

impl std::hash::Hash for NativeFontKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // A PostScript name does not identify a variable font instance.
        // CoreText equality/hash retain its axes, features and cascade list.
        state.write_usize(unsafe { core_foundation::base::CFHash(self.0.as_CFTypeRef()) });
    }
}

struct AppleTextSystemState {
    memory_source: MemSource,
    system_source: SystemSource,
    fonts: Vec<FontKitFont>,
    font_selections: HashMap<Font, FontId>,
    font_ids_by_native_font: HashMap<NativeFontKey, FontId>,
    font_ids_by_font_key: HashMap<FontKey, SmallVec<[FontId; 4]>>,
    postscript_names_by_font_id: HashMap<FontId, String>,
    #[cfg(target_os = "macos")]
    css_source_weights: HashMap<FontId, FontWeight>,
}

impl AppleTextSystem {
    /// Create a new AppleTextSystem.
    pub fn new() -> Self {
        Self(RwLock::new(AppleTextSystemState {
            memory_source: MemSource::empty(),
            system_source: SystemSource::new(),
            fonts: Vec::new(),
            font_selections: HashMap::default(),
            font_ids_by_native_font: HashMap::default(),
            font_ids_by_font_key: HashMap::default(),
            postscript_names_by_font_id: HashMap::default(),
            #[cfg(target_os = "macos")]
            css_source_weights: HashMap::default(),
        }))
    }
}

impl Default for AppleTextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTextSystem for AppleTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.0.write().add_fonts(fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let collection = core_text::font_collection::create_for_all_families();
        // NOTE: We intentionally avoid using `collection.get_descriptors()` here because
        // it has a memory leak bug in core-text v21.0.0. The upstream code uses
        // `wrap_under_get_rule` but `CTFontCollectionCreateMatchingFontDescriptors`
        // follows the Create Rule (caller owns the result), so it should use
        // `wrap_under_create_rule`. We call the function directly with correct memory management.
        unsafe extern "C" {
            fn CTFontCollectionCreateMatchingFontDescriptors(
                collection: CTFontCollectionRef,
            ) -> CFArrayRef;
        }
        let descriptors: Option<CFArray<CTFontDescriptor>> = unsafe {
            let array_ref =
                CTFontCollectionCreateMatchingFontDescriptors(collection.as_concrete_TypeRef());
            if array_ref.is_null() {
                None
            } else {
                Some(CFArray::wrap_under_create_rule(array_ref))
            }
        };
        let Some(descriptors) = descriptors else {
            return names;
        };
        for descriptor in descriptors.into_iter() {
            names.extend(lenient_font_attributes::family_name(&descriptor));
        }
        if let Ok(fonts_in_memory) = self.0.read().memory_source.all_families() {
            names.extend(fonts_in_memory);
        }
        names
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let lock = self.0.upgradable_read();
        if let Some(font_id) = lock.font_selections.get(font) {
            Ok(*font_id)
        } else {
            let mut lock = RwLockUpgradableReadGuard::upgrade(lock);
            let font_key = FontKey {
                font_family: font.family.clone(),
                font_features: font.features.clone(),
                font_fallbacks: font.fallbacks.clone(),
            };
            let candidates = if let Some(font_ids) = lock.font_ids_by_font_key.get(&font_key) {
                font_ids.as_slice()
            } else {
                let font_ids =
                    lock.load_family(&font.family, &font.features, font.fallbacks.as_ref())?;
                lock.font_ids_by_font_key.insert(font_key.clone(), font_ids);
                lock.font_ids_by_font_key[&font_key].as_ref()
            };

            let candidate_properties = candidates
                .iter()
                .map(|font_id| lock.fonts[font_id.0].properties())
                .collect::<SmallVec<[_; 4]>>();

            let ix = font_kit::matching::find_best_match(
                &candidate_properties,
                &font_kit::properties::Properties {
                    style: fontkit_style(font.style),
                    weight: fontkit_weight(font.weight),
                    stretch: Default::default(),
                },
            )?;

            let font_id = candidates[ix];
            #[cfg(target_os = "macos")]
            let system_ui_variant = if font.weight != FontWeight::NORMAL
                && gpui::font_name_with_fallbacks(&font.family, ".AppleSystemUIFont")
                    == ".AppleSystemUIFont"
            {
                // Keep the selected face's fallback weight/style, then apply
                // the exact CSS axis to its Latin variable font.
                system_ui_css_weight(&lock.fonts[font_id.0], font.weight)
            } else {
                None
            };
            #[cfg(target_os = "macos")]
            let font_id = system_ui_variant.map_or(font_id, |font| lock.cache_font(font));
            #[cfg(target_os = "macos")]
            let font_id = if font.weight != FontWeight::NORMAL
                && gpui::font_name_with_fallbacks(&font.family, ".AppleSystemUIFont")
                    != ".AppleSystemUIFont"
            {
                // A local face can match several CSS weights. Retain the
                // requested weight independently so its variable fallback
                // does not inherit only the named primary face's weight.
                let selected = lock.fonts[font_id.0].clone();
                let selected = font_with_css_weight(&selected, font.weight, true)
                    .unwrap_or(selected);
                lock.cache_css_source_font(selected, font.weight)
            } else {
                font_id
            };
            lock.font_selections.insert(font.clone(), font_id);
            Ok(font_id)
        }
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        font_kit_metrics_to_metrics(self.0.read().fonts[font_id.0].metrics())
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        Ok(bounds_from_rect(
            self.0.read().fonts[font_id.0].typographic_bounds(glyph_id.0)?,
        ))
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.0.read().advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.read().glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.0.read().raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        glyph_id: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.0.read().rasterize_glyph(glyph_id, raster_bounds)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        self.0.write().layout_line(text, font_size, font_runs)
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        TextRenderingMode::Grayscale
    }

    #[cfg(target_os = "macos")]
    fn glyph_dilation_for_color(&self, color: Hsla) -> u8 {
        // When font smoothing is enabled, CoreGraphics thickens glyph strokes by an amount that
        // depends on the foreground color's luminance. We replicate the logic used by CoreGraphics
        // to select between the different levels of dilation.
        if !font_smoothing_allowed_by_user() {
            return 0;
        }
        let rgba: Rgba = color.into();
        let luminance = 0.2126 * rgba.r + 0.7152 * rgba.g + 0.0722 * rgba.b;
        let level = ((4.0 * luminance) + 0.5).floor() as i32;
        level.clamp(0, 4) as u8
    }
}

#[cfg(target_os = "macos")]
fn font_smoothing_allowed_by_user() -> bool {
    static ALLOWED: OnceLock<bool> = OnceLock::new();
    *ALLOWED.get_or_init(|| {
        use core_foundation_sys::preferences::{
            CFPreferencesCopyAppValue, kCFPreferencesCurrentApplication,
        };

        let key = CFString::new("AppleFontSmoothing");
        let value_ref = unsafe {
            CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), kCFPreferencesCurrentApplication)
        };
        if value_ref.is_null() {
            return true;
        }
        let value = unsafe { CFType::wrap_under_create_rule(value_ref) };
        let Some(number) = value.downcast_into::<CFNumber>() else {
            return true;
        };
        // Only an explicit value of `0` means that font smoothing is disabled.
        number.to_i64() != Some(0)
    })
}

impl AppleTextSystemState {
    fn add_fonts(&mut self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        let fonts = fonts
            .into_iter()
            .map(|bytes| match bytes {
                Cow::Borrowed(embedded_font) => {
                    let data_provider = unsafe {
                        core_graphics::data_provider::CGDataProvider::from_slice(embedded_font)
                    };
                    let font = core_graphics::font::CGFont::from_data_provider(data_provider)
                        .map_err(|()| anyhow!("Could not load an embedded font."))?;
                    let font = font_kit::loaders::core_text::Font::from_core_graphics_font(font);
                    Ok(Handle::from_native(&font))
                }
                Cow::Owned(bytes) => Ok(Handle::from_memory(Arc::new(bytes), 0)),
            })
            .collect::<Result<Vec<_>>>()?;
        self.memory_source.add_fonts(fonts.into_iter())?;
        Ok(())
    }

    fn load_family(
        &mut self,
        name: &str,
        features: &FontFeatures,
        fallbacks: Option<&FontFallbacks>,
    ) -> Result<SmallVec<[FontId; 4]>> {
        let name = gpui::font_name_with_fallbacks(name, ".AppleSystemUIFont");

        let mut font_ids = SmallVec::new();
        let mut postscript_names_seen = HashSet::default();
        let family = self
            .memory_source
            .select_family_by_name(name)
            .or_else(|_| self.system_source.select_family_by_name(name))?;
        for font in family.fonts() {
            let mut font = font.load()?;

            apply_features_and_fallbacks(&mut font, features, fallbacks)?;
            // This block contains a precautionary fix to guard against loading fonts
            // that might cause panics due to `.unwrap()`s up the chain.
            {
                // We use the 'm' character for text measurements in various spots
                // (e.g., the editor). However, at time of writing some of those usages
                // will panic if the font has no 'm' glyph.
                //
                // Therefore, we check up front that the font has the necessary glyph.
                let has_m_glyph = font.glyph_for_char('m').is_some();

                // HACK: The 'Segoe Fluent Icons' font does not have an 'm' glyph,
                // but we need to be able to load it for rendering Windows icons in
                // the Storybook (on macOS).
                let is_segoe_fluent_icons = font.full_name() == "Segoe Fluent Icons";

                if !has_m_glyph && !is_segoe_fluent_icons {
                    // I spent far too long trying to track down why a font missing the 'm'
                    // character wasn't loading. This log statement will hopefully save
                    // someone else from suffering the same fate.
                    log::warn!(
                        "font '{}' has no 'm' character and was not loaded",
                        font.full_name()
                    );
                    continue;
                }
            }

            // We've seen a number of panics in production caused by calling font.properties()
            // which unwraps a downcast to CFNumber. This is an attempt to avoid the panic,
            // and to try and identify the incalcitrant font.
            let traits = font.native_font().all_traits();
            if unsafe {
                !(traits
                    .get(kCTFontSymbolicTrait)
                    .downcast::<CFNumber>()
                    .is_some()
                    && traits
                        .get(kCTFontWidthTrait)
                        .downcast::<CFNumber>()
                        .is_some()
                    && traits
                        .get(kCTFontWeightTrait)
                        .downcast::<CFNumber>()
                        .is_some()
                    && traits
                        .get(kCTFontSlantTrait)
                        .downcast::<CFNumber>()
                        .is_some())
            } {
                log::error!(
                    "Failed to read traits for font {:?} (PostScript name {:?})",
                    font.full_name(),
                    font.postscript_name(),
                );
                continue;
            }

            let Some(postscript_name) = font.postscript_name() else {
                log::warn!(
                    "font {:?} in family {:?} has no PostScript name; skipping",
                    font.full_name(),
                    name,
                );
                continue;
            };
            // Keep a result for each FontKey, including when a family has
            // already been loaded with different features or fallbacks.
            if !postscript_names_seen.insert(postscript_name.clone()) {
                log::warn!(
                    "skipping duplicate font {:?} with PostScript name {:?} \
                     in family {:?}",
                    font.full_name(),
                    postscript_name,
                    name,
                );
                continue;
            }
            font_ids.push(self.cache_font(font));
        }
        Ok(font_ids)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        Ok(size_from_vector2f(
            self.fonts[font_id.0].advance(glyph_id.0)?,
        ))
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.fonts[font_id.0].glyph_for_char(ch).map(GlyphId)
    }

    fn cache_font(&mut self, font: FontKitFont) -> FontId {
        // FontId is independent of the requested point size. Normalizing the
        // native key retains variation axes without caching each shaped size.
        let key = NativeFontKey(font.native_font().clone_with_font_size(16.));
        if let Some(id) = self.font_ids_by_native_font.get(&key) {
            return *id;
        }
        let id = FontId(self.fonts.len());
        self.postscript_names_by_font_id
            .insert(id, font.native_font().postscript_name());
        self.font_ids_by_native_font.insert(key, id);
        self.fonts.push(font);
        id
    }

    #[cfg(target_os = "macos")]
    fn cache_css_source_font(&mut self, font: FontKitFont, weight: FontWeight) -> FontId {
        // FontRun carries FontId, not the original CSS description. These
        // style IDs must remain distinct even when a fixed primary face is
        // shared. Actual glyph faces still use the native-font cache.
        let id = FontId(self.fonts.len());
        self.postscript_names_by_font_id.insert(id, font.native_font().postscript_name());
        self.fonts.push(font);
        self.css_source_weights.insert(id, weight);
        id
    }

    fn id_for_native_font(&mut self, requested_font: CTFont) -> FontId {
        // Copying through CGFont loses native variation/feature attributes.
        let native = requested_font.clone_with_font_size(16.);
        self.cache_font(unsafe { FontKitFont::from_native_font(&native) })
    }

    fn is_emoji(&self, font_id: FontId) -> bool {
        self.postscript_names_by_font_id
            .get(&font_id)
            .is_some_and(|postscript_name| {
                postscript_name == "AppleColorEmoji" || postscript_name == ".AppleColorEmojiUI"
            })
    }

    fn raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        let font = &self.fonts[params.font_id.0];
        let scale = Transform2F::from_scale(params.scale_factor);
        let bounds: Bounds<DevicePixels> = bounds_from_rect_i(font.raster_bounds(
            params.glyph_id.0,
            params.font_size.into(),
            scale,
            HintingOptions::None,
            font_kit::canvas::RasterizationOptions::GrayscaleAa,
        )?);

        // Expand the bounds by 1 pixel on each side to give CG room for anti-aliasing.
        Ok(bounds.dilate(DevicePixels(1)))
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            anyhow::bail!("glyph bounds are empty");
        } else {
            // Add an extra pixel when the subpixel variant isn't zero to make room for anti-aliasing.
            let mut bitmap_size = glyph_bounds.size;
            if params.subpixel_variant.x > 0 {
                bitmap_size.width += DevicePixels(1);
            }
            if params.subpixel_variant.y > 0 {
                bitmap_size.height += DevicePixels(1);
            }
            let bitmap_size = bitmap_size;

            let mut bytes;
            let cx;
            if params.is_emoji {
                bytes = vec![0; bitmap_size.width.0 as usize * 4 * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize * 4,
                    &CGColorSpace::create_device_rgb(),
                    kCGImageAlphaPremultipliedLast,
                );
            } else {
                bytes = vec![0; bitmap_size.width.0 as usize * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize,
                    &CGColorSpace::create_device_gray(),
                    kCGImageAlphaOnly,
                );
            }

            // Move the origin to bottom left and account for scaling, this
            // makes drawing text consistent with the font-kit's raster_bounds.
            cx.translate(
                -glyph_bounds.origin.x.0 as CGFloat,
                (glyph_bounds.origin.y.0 + glyph_bounds.size.height.0) as CGFloat,
            );
            cx.scale(
                params.scale_factor as CGFloat,
                params.scale_factor as CGFloat,
            );

            let subpixel_shift = params
                .subpixel_variant
                .map(|v| v as f32 / SUBPIXEL_VARIANTS_X as f32);
            cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
            cx.set_allows_antialiasing(true);
            cx.set_should_antialias(true);
            cx.set_allows_font_subpixel_positioning(true);
            cx.set_should_subpixel_position_fonts(true);
            cx.set_allows_font_subpixel_quantization(false);
            cx.set_should_subpixel_quantize_fonts(false);

            if cfg!(target_os = "macos") && params.dilation > 0 {
                let luminance = params.dilation as f64 * 0.25;
                cx.set_should_smooth_fonts(true);
                cx.set_gray_fill_color(luminance, 1.0);
            } else {
                cx.set_gray_fill_color(0.0, 1.0);
            }
            self.fonts[params.font_id.0]
                .native_font()
                .clone_with_font_size(f32::from(params.font_size) as CGFloat)
                .draw_glyphs(
                    &[params.glyph_id.0 as CGGlyph],
                    &[CGPoint::new(
                        (subpixel_shift.x / params.scale_factor) as CGFloat,
                        (subpixel_shift.y / params.scale_factor) as CGFloat,
                    )],
                    cx,
                );

            if params.is_emoji {
                // Convert from RGBA with premultiplied alpha to BGRA with straight alpha.
                for pixel in bytes.chunks_exact_mut(4) {
                    swap_rgba_pa_to_bgra(pixel);
                }
            }

            Ok((bitmap_size, bytes))
        }
    }

    fn layout_line(&mut self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        // Construct the attributed string, converting UTF8 ranges to UTF16 ranges.
        let mut string = CFMutableAttributedString::new();
        let mut max_ascent = 0.0f32;
        let mut max_descent = 0.0f32;
        #[cfg(target_os = "macos")]
        let mut ui_font_sources = Vec::new();
        #[cfg(target_os = "macos")]
        let mut local_font_sources = Vec::new();

        {
            let mut text = text;
            let mut break_ligature = true;
            for run in font_runs {
                let text_run;
                (text_run, text) = text.split_at(run.len);

                let utf16_start = string.char_len(); // insert at end of string
                // note: replace_str may silently ignore codepoints it dislikes (e.g., BOM at start of string)
                string.replace_str(&CFString::new(text_run), CFRange::init(utf16_start, 0));
                let utf16_end = string.char_len();

                let length = utf16_end - utf16_start;
                let cf_range = CFRange::init(utf16_start, length);
                let font = &self.fonts[run.font_id.0];

                let font_metrics = font.metrics();
                let font_scale = f32::from(font_size) / font_metrics.units_per_em as f32;
                let ascent = font_metrics.ascent * font_scale;
                let descent = -font_metrics.descent * font_scale;
                let native = font.native_font();
                #[cfg(target_os = "macos")]
                let family = native.family_name();
                #[cfg(target_os = "macos")]
                let ascent = if ["Times", "Helvetica", "Courier"]
                    .iter().any(|name| family.eq_ignore_ascii_case(name))
                {
                    // Match WebKit's local-font vertical metrics at this size.
                    // See WebKit-Font-Metrics-LICENSE.txt and FontCoreText.cpp.
                    ascent + ((ascent + descent) * 0.15).round()
                } else {
                    ascent
                };
                max_ascent = max_ascent.max(ascent);
                max_descent = max_descent.max(descent);

                #[cfg(target_os = "macos")]
                let logical_size = font_size;
                let font_size = if break_ligature && font_runs.len() > 1 {
                    px(f32::from(font_size).next_up())
                } else {
                    font_size
                };
                #[cfg(target_os = "macos")]
                if family == ".AppleSystemUIFont" {
                    ui_font_sources.push((cf_range, native.clone()));
                }
                #[cfg(target_os = "macos")]
                if let Some(weight) = self.css_source_weights.get(&run.font_id) {
                    local_font_sources.push((cf_range, family.clone(), *weight));
                }
                let sized = native.clone_with_font_size(font_size.into());
                #[cfg(target_os = "macos")]
                let sized = if font_size != logical_size
                    && family == ".AppleSystemUIFont"
                {
                    // Retain the boundary's distinct font size, but keep the
                    // UI font's optical metrics at the requested CSS size.
                    // Even a one-ULP increase can cross a CoreText tracking step.
                    use core_foundation::dictionary::CFDictionary;
                    unsafe extern "C" {
                        static kCTFontOpticalSizeAttribute:
                            core_foundation::string::CFStringRef;
                    }
                    let key = unsafe { CFString::wrap_under_get_rule(kCTFontOpticalSizeAttribute) };
                    let attributes = CFDictionary::from_CFType_pairs(&[(
                        key, CFNumber::from(f64::from(f32::from(logical_size))).as_CFType(),
                    )]);
                    native_with_attributes(&sized, &attributes)
                } else {
                    sized
                };
                unsafe {
                    string.set_attribute(
                        cf_range,
                        kCTFontAttributeName,
                        &sized,
                    );
                }
                break_ligature = !break_ligature;
            }
        }
        // Retrieve the glyphs from the shaped line, converting UTF16 offsets to UTF8 offsets.
        let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
        #[cfg(target_os = "macos")]
        let line = if preserve_ui_text_fallback_faces(
            &mut string, &line, &ui_font_sources, f64::from(f32::from(font_size)),
        ) {
            CTLine::new_with_attributed_string(string.as_concrete_TypeRef())
        } else {
            line
        };
        #[cfg(target_os = "macos")]
        let line = if preserve_local_css_fallback_weights(&mut string, &line, &local_font_sources) {
            CTLine::new_with_attributed_string(string.as_concrete_TypeRef())
        } else {
            line
        };
        let glyph_runs = line.glyph_runs();
        let mut runs = <Vec<ShapedRun>>::with_capacity(glyph_runs.len() as usize);
        let mut ix_converter = StringIndexConverter::new(text);
        for run in glyph_runs.into_iter() {
            let attributes = run.attributes().unwrap();
            let font = unsafe {
                attributes
                    .get(kCTFontAttributeName)
                    .downcast::<CTFont>()
                    .unwrap()
            };
            let font_id = self.id_for_native_font(font);

            let glyphs = match runs.last_mut() {
                Some(run) if run.font_id == font_id => &mut run.glyphs,
                _ => {
                    runs.push(ShapedRun {
                        font_id,
                        glyphs: Vec::with_capacity(run.glyph_count().try_into().unwrap_or(0)),
                    });
                    &mut runs.last_mut().unwrap().glyphs
                }
            };
            for ((&glyph_id, position), &glyph_utf16_ix) in run
                .glyphs()
                .iter()
                .zip(run.positions().iter())
                .zip(run.string_indices().iter())
            {
                let glyph_utf16_ix = usize::try_from(glyph_utf16_ix).unwrap();
                if ix_converter.utf16_ix > glyph_utf16_ix {
                    // We cannot reuse current index converter, as it can only seek forward. Restart the search.
                    ix_converter = StringIndexConverter::new(text);
                }
                ix_converter.advance_to_utf16_ix(glyph_utf16_ix);
                glyphs.push(ShapedGlyph {
                    id: GlyphId(glyph_id as u32),
                    position: point(position.x as f32, position.y as f32).map(px),
                    index: ix_converter.utf8_ix,
                    is_emoji: self.is_emoji(font_id),
                });
            }
        }
        let typographic_bounds = line.get_typographic_bounds();
        LineLayout {
            runs,
            font_size,
            width: typographic_bounds.width.into(),
            ascent: max_ascent.into(),
            descent: max_descent.into(),
            len: text.len(),
        }
    }
}

#[derive(Debug, Clone)]
struct StringIndexConverter<'a> {
    text: &'a str,
    /// Index in UTF-8 bytes
    utf8_ix: usize,
    /// Index in UTF-16 code units
    utf16_ix: usize,
}

impl<'a> StringIndexConverter<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            utf8_ix: 0,
            utf16_ix: 0,
        }
    }

    fn advance_to_utf16_ix(&mut self, utf16_target: usize) {
        for (ix, c) in self.text[self.utf8_ix..].char_indices() {
            if self.utf16_ix >= utf16_target {
                self.utf8_ix += ix;
                return;
            }
            self.utf16_ix += c.len_utf16();
        }
        self.utf8_ix = self.text.len();
    }
}

fn font_kit_metrics_to_metrics(metrics: Metrics) -> FontMetrics {
    FontMetrics {
        units_per_em: metrics.units_per_em,
        ascent: metrics.ascent,
        descent: metrics.descent,
        line_gap: metrics.line_gap,
        underline_position: metrics.underline_position,
        underline_thickness: metrics.underline_thickness,
        cap_height: metrics.cap_height,
        x_height: metrics.x_height,
        bounding_box: bounds_from_rect(metrics.bounding_box),
    }
}

fn bounds_from_rect(rect: RectF) -> Bounds<f32> {
    Bounds {
        origin: point(rect.origin_x(), rect.origin_y()),
        size: size(rect.width(), rect.height()),
    }
}

fn bounds_from_rect_i(rect: RectI) -> Bounds<DevicePixels> {
    Bounds {
        origin: point(DevicePixels(rect.origin_x()), DevicePixels(rect.origin_y())),
        size: size(DevicePixels(rect.width()), DevicePixels(rect.height())),
    }
}

// impl From<Vector2I> for Size<DevicePixels> {
//     fn from(value: Vector2I) -> Self {
//         size(value.x().into(), value.y().into())
//     }
// }

// impl From<RectI> for Bounds<i32> {
//     fn from(rect: RectI) -> Self {
//         Bounds {
//             origin: point(rect.origin_x(), rect.origin_y()),
//             size: size(rect.width(), rect.height()),
//         }
//     }
// }

// impl From<Point<u32>> for Vector2I {
//     fn from(size: Point<u32>) -> Self {
//         Vector2I::new(size.x as i32, size.y as i32)
//     }
// }

fn size_from_vector2f(vec: Vector2F) -> Size<f32> {
    size(vec.x(), vec.y())
}

#[cfg(target_os = "macos")]
fn preserve_ui_text_fallback_faces(
    string: &mut CFMutableAttributedString,
    line: &CTLine,
    sources: &[(CFRange, CTFont)],
    size: f64,
) -> bool {
    use core_foundation::attributed_string::CFAttributedStringGetString;
    unsafe extern "C" {
        fn CTRunGetStringRange(run: core_text::run::CTRunRef) -> CFRange;
    }
    let contains = |outer: CFRange, inner: CFRange| {
        outer.location <= inner.location
            && outer.location + outer.length >= inner.location + inner.length
    };
    let mut changed = false;
    for run in line.glyph_runs().iter() {
        let Some(attributes) = run.attributes() else { continue; };
        let Some(fallback) = (unsafe { attributes.get(kCTFontAttributeName).downcast::<CTFont>() })
            else { continue; };
        if !fallback.postscript_name().starts_with(".PingFangUIDisplay") { continue; }
        let range = unsafe { CTRunGetStringRange(run.as_concrete_TypeRef()) };
        let Some((_, source)) = sources.iter().find(|(owner, _)| contains(*owner, range))
            else { continue; };

        // Resolve the original text face before resizing. Copying the native
        // UI font to a larger size reselects its Display design. Its CGFont
        // retains the selected Text glyphs and variable weight instead.
        let plain = unsafe { CFString::wrap_under_get_rule(
            CFAttributedStringGetString(string.as_concrete_TypeRef()),
        ) };
        let mut base = CFMutableAttributedString::new();
        base.replace_str(&plain, CFRange::init(0, 0));
        unsafe { base.set_attribute(CFRange::init(0, base.char_len()), kCTFontAttributeName, source); }
        let base_line = CTLine::new_with_attributed_string(base.as_concrete_TypeRef());
        for candidate in base_line.glyph_runs().iter() {
            let candidate_range = unsafe { CTRunGetStringRange(candidate.as_concrete_TypeRef()) };
            if !contains(candidate_range, range) { continue; }
            let Some(attributes) = candidate.attributes() else { continue; };
            let Some(chosen) = (unsafe { attributes.get(kCTFontAttributeName).downcast::<CTFont>() })
                else { continue; };
            if !chosen.postscript_name().starts_with(".PingFangUIText") { continue; }
            let face = core_text::font::new_from_CGFont(&chosen.copy_to_CGFont(), size);
            unsafe { string.set_attribute(range, kCTFontAttributeName, &face); }
            changed = true;
            break;
        }
    }
    changed
}

#[cfg(target_os = "macos")]
fn system_ui_css_weight(font: &FontKitFont, weight: FontWeight) -> Option<FontKitFont> {
    font_with_css_weight(font, weight, false)
}

#[cfg(target_os = "macos")]
fn preserve_local_css_fallback_weights(
    string: &mut CFMutableAttributedString,
    line: &CTLine,
    sources: &[(CFRange, String, FontWeight)],
) -> bool {
    unsafe extern "C" {
        fn CTRunGetStringRange(run: core_text::run::CTRunRef) -> CFRange;
    }
    let mut changed = false;
    for run in line.glyph_runs().iter() {
        let range = unsafe { CTRunGetStringRange(run.as_concrete_TypeRef()) };
        let Some((_, family, weight)) = sources.iter().find(|(owner, _, _)| {
            owner.location <= range.location
                && owner.location + owner.length >= range.location + range.length
        }) else { continue; };
        let Some(attributes) = run.attributes() else { continue; };
        let Some(native) = (unsafe { attributes.get(kCTFontAttributeName).downcast::<CTFont>() })
            else { continue; };
        if native.family_name() == *family { continue; }
        let face = unsafe { FontKitFont::from_native_font(&native) };
        let Some(varied) = font_with_css_weight(&face, *weight, true) else { continue; };
        unsafe { string.set_attribute(range, kCTFontAttributeName, &varied.native_font()); }
        changed = true;
    }
    changed
}

#[cfg(target_os = "macos")]
fn font_with_css_weight(font: &FontKitFont, weight: FontWeight, preserve_axes: bool) -> Option<FontKitFont> {
    use core_foundation::{dictionary::CFDictionary, string::CFStringRef};
    use core_text::font_descriptor::kCTFontVariationAttribute;

    unsafe extern "C" {
        static kCTFontVariationAxisIdentifierKey: CFStringRef;
        static kCTFontVariationAxisMinimumValueKey: CFStringRef;
        static kCTFontVariationAxisMaximumValueKey: CFStringRef;
    }
    const WEIGHT_AXIS: i32 = i32::from_be_bytes(*b"wght");

    let native = font.native_font();
    let native = if preserve_axes && native.get_variation_axes().is_none() {
        variable_family_face(&native).unwrap_or(native)
    } else {
        native
    };
    let axes = native.get_variation_axes()?;
    let axis = axes.iter().find(|axis| unsafe {
        axis.find(kCTFontVariationAxisIdentifierKey)
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|value| value.to_i32()) == Some(WEIGHT_AXIS)
    })?;
    let number = |key| {
        axis.find(key)?.downcast::<CFNumber>()?.to_f64()
    };
    let min = number(unsafe { kCTFontVariationAxisMinimumValueKey })?;
    let max = number(unsafe { kCTFontVariationAxisMaximumValueKey })?;
    if !weight.0.is_finite() {
        return None;
    }
    // CSS weights address the variable axis directly. Named AppKit Heavy and
    // Black instances use 860 and 1000 here, rather than CSS 800 and 900.
    let mut pairs = Vec::new();
    if preserve_axes {
        unsafe extern "C" {
            fn CTFontCopyVariation(font: core_text::font::CTFontRef)
                -> core_foundation::dictionary::CFDictionaryRef;
        }
        let current = unsafe { CTFontCopyVariation(native.as_concrete_TypeRef()) };
        if !current.is_null() {
            let current = unsafe { CFDictionary::<CFNumber, CFNumber>::wrap_under_create_rule(current) };
            let (keys, values) = current.get_keys_and_values();
            for (key, value) in keys.into_iter().zip(values) {
                let key = unsafe { CFNumber::wrap_under_get_rule(key.cast()) };
                if key.to_i32() != Some(WEIGHT_AXIS) {
                    let value = unsafe { CFNumber::wrap_under_get_rule(value.cast()) };
                    pairs.push((key, value));
                }
            }
        }
    }
    pairs.push((
        CFNumber::from(WEIGHT_AXIS),
        CFNumber::from((weight.0 as f64).clamp(min, max)),
    ));
    let variation = CFDictionary::from_CFType_pairs(&pairs);
    let key = unsafe { CFString::wrap_under_get_rule(kCTFontVariationAttribute) };
    let attributes = CFDictionary::from_CFType_pairs(&[(key, variation.as_CFType())]);
    Some(unsafe { FontKitFont::from_native_font(&native_with_attributes(&native, &attributes)) })
}

#[cfg(target_os = "macos")]
fn variable_family_face(native: &CTFont) -> Option<CTFont> {
    use core_foundation::dictionary::CFDictionary;
    use core_text::font_descriptor::kCTFontURLAttribute;

    // CoreText can resolve a PostScript fallback to a static downloadable
    // face while resolving its family to a variable system collection.
    // Retain the exact named instance and its attributes, using only the
    // source returned by the public family lookup. No font path is fixed.
    let family = core_text::font::new_from_name(&native.family_name(), native.pt_size()).ok()?;
    if family.family_name() != native.family_name() || family.get_variation_axes().is_none() {
        return None;
    }
    let url = family.url()?;
    let attributes = native.copy_descriptor().attributes();
    let url_key = unsafe { CFString::wrap_under_get_rule(kCTFontURLAttribute) };
    let (keys, values) = attributes.get_keys_and_values();
    let mut pairs = Vec::with_capacity(keys.len() + 1);
    for (key, value) in keys.into_iter().zip(values) {
        let key = unsafe { CFString::wrap_under_get_rule(key.cast()) };
        if key != url_key {
            pairs.push((key, unsafe { CFType::wrap_under_get_rule(value.cast()) }));
        }
    }
    pairs.push((url_key, url.as_CFType()));
    let attributes = CFDictionary::from_CFType_pairs(&pairs);
    let descriptor = core_text::font_descriptor::new_from_attributes(&attributes);
    let candidate = core_text::font::new_from_descriptor(&descriptor, native.pt_size());
    (candidate.family_name() == native.family_name()
        && candidate.postscript_name() == native.postscript_name()
        && candidate.get_variation_axes().is_some()).then_some(candidate)
}

#[cfg(target_os = "macos")]
fn native_with_attributes(
    native: &CTFont,
    attributes: &core_foundation::dictionary::CFDictionary<CFString, CFType>,
) -> CTFont {
    let descriptor = core_text::font_descriptor::new_from_attributes(&attributes);
    unsafe {
        let generated_font = &*native.as_concrete_TypeRef().cast::<objc2_core_text::CTFont>();
        let descriptor = &*descriptor.as_concrete_TypeRef()
            .cast::<objc2_core_text::CTFontDescriptor>();
        let varied = generated_font.copy_with_attributes(0., std::ptr::null(), Some(descriptor));
        CTFont::wrap_under_create_rule(
            objc2_core_foundation::CFRetained::into_raw(varied).as_ptr().cast(),
        )
    }
}

fn fontkit_weight(value: FontWeight) -> FontkitWeight {
    FontkitWeight(value.0)
}

fn fontkit_style(style: FontStyle) -> FontkitStyle {
    match style {
        FontStyle::Normal => FontkitStyle::Normal,
        FontStyle::Italic => FontkitStyle::Italic,
        FontStyle::Oblique => FontkitStyle::Oblique,
    }
}

// Some fonts may have no attributes despite `core_text` requiring them (and panicking).
// This is the same version as `core_text` has without `expect` calls.
mod lenient_font_attributes {
    use core_foundation::{
        base::{CFRetain, CFType, TCFType},
        string::{CFString, CFStringRef},
    };
    use core_text::font_descriptor::{
        CTFontDescriptor, CTFontDescriptorCopyAttribute, kCTFontFamilyNameAttribute,
    };

    pub fn family_name(descriptor: &CTFontDescriptor) -> Option<String> {
        unsafe { get_string_attribute(descriptor, kCTFontFamilyNameAttribute) }
    }

    fn get_string_attribute(
        descriptor: &CTFontDescriptor,
        attribute: CFStringRef,
    ) -> Option<String> {
        unsafe {
            let value = CTFontDescriptorCopyAttribute(descriptor.as_concrete_TypeRef(), attribute);
            if value.is_null() {
                return None;
            }

            let value = CFType::wrap_under_create_rule(value);
            assert!(value.instance_of::<CFString>());
            let s = wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef);
            Some(s.to_string())
        }
    }

    unsafe fn wrap_under_get_rule(reference: CFStringRef) -> CFString {
        unsafe {
            assert!(!reference.is_null(), "Attempted to create a NULL object.");
            let reference = CFRetain(reference as *const ::std::os::raw::c_void) as CFStringRef;
            TCFType::wrap_under_create_rule(reference)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::AppleTextSystem;
    use gpui::{FontFallbacks, FontFeatures, FontRun, GlyphId, PlatformTextSystem, font, px};
    use std::sync::Arc;

    #[cfg(target_os = "macos")]
    #[test]
    fn rdm_local_css_fallback_weights_match_original() {
        // Independent original WKWebView spans,16px,from
        // tests/fixtures/status-font-synthesis-macos-source.json. CSS
        // font-synthesis:none keeps these widths;no pixel offset is fitted.
        let fonts = AppleTextSystem::new();
        let mut mismatches = Vec::new();
        for family in ["Courier", "Arial", ".SystemUIFont"] {
            for weight in [400., 450., 500., 550., 600., 700.] {
                let mut description = font(family);
                description.weight = gpui::FontWeight(weight);
                let id = fonts.font_id(&description).unwrap();
                for text in ["活动状态", "状态信息", "键", "值"] {
                    let expected = if family == ".SystemUIFont" {
                        if text.chars().count() == 1 { 15.890625 } else { 63.515625 }
                    } else if (450. ..=550.).contains(&weight) {
                        if text.chars().count() == 1 { 16.328125 } else { 65.28125 }
                    } else if text.chars().count() == 1 { 16. } else { 64. };
                    let actual = fonts.layout_line(text, px(16.), &[FontRun { font_id:id, len:text.len() }]).width.as_f32();
                    println!("LOCAL CSS FALLBACK: family={family:?} weight={weight} text={text:?} actual={actual} expected={expected}");
                    if (actual - expected).abs() >= 0.02 {
                        mismatches.push((family, weight, text, actual, expected));
                    }
                }
            }
        }
        assert!(mismatches.is_empty(), "Original local-font fallback widths: {mismatches:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rdm_webkit_font_boxes_match_css_source_samples() {
        // Independent WKWebView canvas font boxes, including unaffected controls.
        // tests/fixtures/css-font-metrics-macos-source.json pins the raw capture.
        let samples = [
            ("Times", 9.0, 8.0, 2.0),
            ("Times", 10.0, 10.0, 3.0),
            ("Times", 12.0, 11.0, 3.0),
            ("Times", 14.0, 13.0, 4.0),
            ("Times", 16.0, 14.0, 4.0),
            ("Times", 20.0, 18.0, 5.0),
            ("Times", 24.0, 22.0, 6.0),
            ("Helvetica", 9.0, 8.0, 2.0),
            ("Helvetica", 10.0, 10.0, 2.0),
            ("Helvetica", 12.0, 11.0, 3.0),
            ("Helvetica", 14.0, 13.0, 3.0),
            ("Helvetica", 16.0, 14.0, 4.0),
            ("Helvetica", 20.0, 18.0, 5.0),
            ("Helvetica", 24.0, 22.0, 6.0),
            ("Courier", 9.0, 8.0, 2.0),
            ("Courier", 10.0, 10.0, 2.0),
            ("Courier", 12.0, 11.0, 3.0),
            ("Courier", 14.0, 13.0, 3.0),
            ("Courier", 16.0, 14.0, 4.0),
            ("Courier", 20.0, 18.0, 5.0),
            ("Courier", 24.0, 22.0, 6.0),
            ("Times New Roman", 9.0, 8.0, 2.0),
            ("Times New Roman", 10.0, 9.0, 2.0),
            ("Times New Roman", 12.0, 11.0, 3.0),
            ("Times New Roman", 14.0, 12.0, 3.0),
            ("Times New Roman", 16.0, 14.0, 3.0),
            ("Times New Roman", 20.0, 18.0, 4.0),
            ("Times New Roman", 24.0, 21.0, 5.0),
            ("Arial", 9.0, 8.0, 2.0),
            ("Arial", 10.0, 9.0, 2.0),
            ("Arial", 12.0, 11.0, 3.0),
            ("Arial", 14.0, 13.0, 3.0),
            ("Arial", 16.0, 14.0, 3.0),
            ("Arial", 20.0, 18.0, 4.0),
            ("Arial", 24.0, 22.0, 5.0),
            ("Menlo", 9.0, 8.0, 2.0),
            ("Menlo", 10.0, 9.0, 2.0),
            ("Menlo", 12.0, 11.0, 3.0),
            ("Menlo", 14.0, 13.0, 3.0),
            ("Menlo", 16.0, 15.0, 4.0),
            ("Menlo", 20.0, 19.0, 5.0),
            ("Menlo", 24.0, 22.0, 6.0),
            (".SystemUIFont", 9.0, 9.0, 2.0),
            (".SystemUIFont", 10.0, 10.0, 2.0),
            (".SystemUIFont", 12.0, 12.0, 3.0),
            (".SystemUIFont", 14.0, 14.0, 3.0),
            (".SystemUIFont", 16.0, 15.0, 3.0),
            (".SystemUIFont", 20.0, 19.0, 4.0),
            (".SystemUIFont", 24.0, 23.0, 5.0),
        ];
        let fonts = AppleTextSystem::new();
        let text = "very-long-redis-search-pattern-1234567890";
        let mut mismatches = Vec::new();
        for (family, size, expected_ascent, expected_descent) in samples {
            let id = fonts.font_id(&font(family)).unwrap();
            let layout = fonts.layout_line(text, px(size), &[FontRun { font_id: id, len: text.len() }]);
            let actual = (layout.ascent.as_f32().round(), layout.descent.as_f32().round());
            let expected = (expected_ascent, expected_descent);
            if actual != expected { mismatches.push((family, size, actual, expected)); }
        }
        assert!(mismatches.is_empty(), "WK font-box mismatches: {mismatches:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rdm_system_ui_css_weights_keep_native_identity() {
        let fonts = AppleTextSystem::new();
        let mut ids = Vec::new();
        // Independent 14px localhost advances from the original body CSS in
        // WKWebView. One DOM layout unit (1/64px) fits the existing 0.02 bound.
        for (weight, expected, expected18) in [
            (100., 56.765625, 70.25), (200., 57.328125, 71.), (300., 57.875, 71.75),
            (400., 58.421875, 72.5), (500., 60.03125, 74.578125), (600., 61.640625, 76.671875),
            (700., 63.265625, 78.765625), (800., 64.875, 80.84375), (900., 66.484375, 82.9375),
        ] {
            let mut font = font(".SystemUIFont");
            font.weight = gpui::FontWeight(weight);
            let id = fonts.font_id(&font).unwrap();
            assert!(!ids.contains(&id), "CSS weight {weight} reused another instance");
            ids.push(id);
            for size in [12., 14., 18., 14.] {
                let line = fonts.layout_line("localhost", px(size), &[FontRun { font_id: id, len: 9 }]);
                assert!(line.runs.iter().all(|run| run.font_id == id),
                    "CSS weight {weight} lost its native instance at {size}px");
                if size == 14. {
                    assert!((line.width - px(expected)).abs() < px(0.02),
                        "CSS weight {weight}: {} vs {expected}", line.width);
                }
                if size == 18. {
                    assert!((line.width - px(expected18)).abs() < px(0.02),
                        "CSS weight {weight}, 18px: {} vs {expected18}", line.width);
                }
            }
            assert_eq!(fonts.font_id(&font).unwrap(), id);
        }
        for (first, second, expected) in [(400., 800., 137.046875), (800., 400., 138.328125)] {
            let mut a = font(".SystemUIFont"); a.weight = gpui::FontWeight(first);
            let mut b = font(".SystemUIFont"); b.weight = gpui::FontWeight(second);
            let runs = [FontRun { font_id: fonts.font_id(&a).unwrap(), len: 10 },
                FontRun { font_id: fonts.font_id(&b).unwrap(), len: 7 }];
            let line = fonts.layout_line("localhost replica", px(18.), &runs);
            assert!((line.width - px(expected)).abs() < px(0.02),
                "mixed CSS {first}/{second}, 18px: {} vs {expected}", line.width);
        }
        for (weight, size, expected) in [
            (400., 24., 136.21875), (800., 24., 142.75),
            (400., 32., 181.3125), (800., 32., 190.28125),
        ] {
            let mut font = font(".SystemUIFont"); font.weight = gpui::FontWeight(weight);
            let text = "中文🙂 alpha";
            let line = fonts.layout_line(text, px(size),
                &[FontRun { font_id: fonts.font_id(&font).unwrap(), len: text.len() }]);
            assert!((line.width - px(expected)).abs() < px(0.02),
                "CSS {weight}, {size}px Unicode: {} vs {expected}", line.width);
            assert!(line.runs.iter().all(|run| !fonts.0.read().fonts[run.font_id.0]
                .native_font().postscript_name().starts_with(".PingFangUIDisplay")));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rdm_emoji_advances_match_macos_webkit() {
        // Independently measured in macOS WKWebView and the original Monaco.
        // Preserve CoreText tracking; Chromium uses different Emoji advances.
        let expected = [
            (12., 16.),
            (13., 17.),
            (14., 19.),
            (15., 20.),
            (18., 22.),
            (24., 25.),
            (32., 32.),
        ];
        let fonts = AppleTextSystem::new();
        for family in ["Menlo", "Monaco", "Courier New"] {
            let font_id = fonts.font_id(&font(family)).unwrap();
            for (size, emoji_width) in expected {
                let shape = |text: &str| {
                    fonts.layout_line(
                        text,
                        px(size),
                        &[FontRun {
                            font_id,
                            len: text.len(),
                        }],
                    )
                };
                let ascii_width = shape("A").width;
                let suffix_width = shape("B").width;
                for text in ["A🙂B", "A👩‍💻B", "A🇨🇳B"] {
                    let line = shape(text);
                    let expected = ascii_width + px(emoji_width) + suffix_width;
                    assert!(
                        (line.width - expected).abs() < px(0.02),
                        "{family} {size} {text}: {} != {}",
                        line.width,
                        expected
                    );
                    assert_eq!(line.len, text.len());
                    let b = line
                        .runs
                        .iter()
                        .flat_map(|run| &run.glyphs)
                        .find(|glyph| glyph.index == text.len() - 1)
                        .unwrap();
                    assert!((b.position.x - ascii_width - px(emoji_width)).abs() < px(0.02));
                }
                let adjacent = shape("🙂🙂");
                assert!((adjacent.width - px(emoji_width * 2.)).abs() < px(0.02));
            }
        }
        // font-kit does not enumerate this hidden family. CoreText fallback
        // registers it through this same native-font path in production.
        let emoji_font = core_text::font::new_from_name("AppleColorEmoji", 14.).unwrap();
        let font_id = fonts.0.write().id_for_native_font(emoji_font);
        let line = fonts.layout_line(
            "🙂",
            px(14.),
            &[FontRun {
                font_id,
                len: "🙂".len(),
            }],
        );
        assert!((line.width - px(19.)).abs() < px(0.02));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rdm_non_emoji_advances_remain_unchanged() {
        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font("Menlo")).unwrap();
        for (text, width) in [
            ("A中B", 30.857421875),
            ("Ae\u{301}B", 25.2861328125),
            ("AΩ·B", 33.71484375),
        ] {
            let line = fonts.layout_line(
                text,
                px(14.),
                &[FontRun {
                    font_id,
                    len: text.len(),
                }],
            );
            assert!(
                (line.width - px(width)).abs() < px(0.02),
                "{text}: {}",
                line.width
            );
        }
    }

    #[test]
    fn test_reload_family_with_features_and_fallbacks() {
        let fonts = AppleTextSystem::new();
        let mut font = font("Helvetica");
        let original_id = fonts.font_id(&font).unwrap();
        font.features = FontFeatures::disable_ligatures();
        font.fallbacks = Some(FontFallbacks(Arc::new(vec!["Times".into()])));
        let configured_id = fonts.font_id(&font).unwrap();
        assert_ne!(original_id, configured_id);
        assert_eq!(fonts.font_id(&font).unwrap(), configured_id);
        assert!(fonts.glyph_for_char(configured_id, 'm').is_some());
        let layout = fonts.layout_line(
            "office",
            px(16.),
            &[FontRun {
                font_id: configured_id,
                len: 6,
            }],
        );
        assert_eq!(layout.len, 6);
        assert!(!layout.runs.is_empty());
    }

    #[test]
    fn test_layout_line_bom_char() {
        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();
        let line = "\u{feff}";
        let mut style = FontRun {
            font_id,
            len: line.len(),
        };

        let layout = fonts.layout_line(line, px(16.), &[style]);
        assert_eq!(layout.len, line.len());
        assert!(layout.runs.is_empty());

        let line = "a\u{feff}b";
        style.len = line.len();
        let layout = fonts.layout_line(line, px(16.), &[style]);
        assert_eq!(layout.len, line.len());
        assert_eq!(layout.runs.len(), 1);
        assert_eq!(layout.runs[0].glyphs.len(), 2);
        assert_eq!(layout.runs[0].glyphs[0].id, GlyphId(68u32)); // a
        // There's no glyph for \u{feff}
        assert_eq!(layout.runs[0].glyphs[1].id, GlyphId(69u32)); // b

        let line = "\u{feff}ab";
        let font_runs = &[
            FontRun {
                len: "\u{feff}".len(),
                font_id,
            },
            FontRun {
                len: "ab".len(),
                font_id,
            },
        ];
        let layout = fonts.layout_line(line, px(16.), font_runs);
        assert_eq!(layout.len, line.len());
        assert_eq!(layout.runs.len(), 1);
        assert_eq!(layout.runs[0].glyphs.len(), 2);
        // There's no glyph for \u{feff}
        assert_eq!(layout.runs[0].glyphs[0].id, GlyphId(68u32)); // a
        assert_eq!(layout.runs[0].glyphs[1].id, GlyphId(69u32)); // b
    }

    #[test]
    fn test_layout_line_zwnj_insertion() {
        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();

        let text = "hello world";
        let font_runs = &[
            FontRun { font_id, len: 5 }, // "hello"
            FontRun { font_id, len: 6 }, // " world"
        ];

        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        for run in &layout.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }

        // Test with different font runs - should not insert ZWNJ
        let font_id2 = fonts.font_id(&font("Times")).unwrap_or(font_id);
        let font_runs_different = &[
            FontRun { font_id, len: 5 }, // "hello"
            // " world"
            FontRun {
                font_id: font_id2,
                len: 6,
            },
        ];

        let layout2 = fonts.layout_line(text, px(16.), font_runs_different);
        assert_eq!(layout2.len, text.len());

        for run in &layout2.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }
    }

    #[test]
    fn test_layout_line_zwnj_edge_cases() {
        let fonts = AppleTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();

        let text = "hello";
        let font_runs = &[FontRun { font_id, len: 5 }];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        let text = "abc";
        let font_runs = &[
            FontRun { font_id, len: 1 }, // "a"
            FontRun { font_id, len: 1 }, // "b"
            FontRun { font_id, len: 1 }, // "c"
        ];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        for run in &layout.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }

        // Test with empty text
        let text = "";
        let font_runs = &[];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, 0);
        assert!(layout.runs.is_empty());
    }
}
