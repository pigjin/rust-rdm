# Local Tiny RDM changes

Based on the unmodified `gpui-pre-apple` 0.3.9 crate, Zed revision
`93d745668805029476e6d7fbb19652116fed51de`. The upstream source remains
under its Apache-2.0 license.

Production retains CoreText shaping, glyph rasterization and optical tracking.
The macOS system UI font now addresses the variable `wght` axis with CSS weights,
because AppKit's named Heavy/Black instances use 860/1000 in the sampled system
font, while original CSS requests 800/900. Font IDs use complete native CoreText
identity at a normalized point size, preserving axes, features and fallbacks;
PostScript names alone alias different instances. Native fallback fonts retain
their attributes instead of being reconstructed through CGFont.

Local tests preserve macOS WKWebView Emoji advances across
Menlo, Monaco, Courier New, seven sizes, ZWJ sequences, flags and adjacent Emoji.
Chinese, combining-character and Greek controls also have regression checks.
The additional CVBase dependency feature is required by the upstream test module.
The weight fixture contains 252 single-font and 14 mixed-weight DOM advances
from the pinned original body CSS in an own hidden WKWebView, across seven
sizes. A single font run keeps its requested size. Mixed runs retain upstream's
size distinction for ligature boundaries, with the macOS UI font's optical
size explicitly kept at the logical size. Independent native measurements
showed that even a one-ULP size increase can cross a tracking step at 18px.
The named-font, resize and optical-attribute probes are archived under
`outputs/source-probes/system-font-weight`.

Larger Chinese UI fallback runs resolve their original text face at the base
font size and retain its CGFont while resizing. A native copy otherwise
rematches the Display design and changes advances. This is limited to the
sampled PingFang UI Text/Display family; the existing non-UI, Emoji and other
fallback paths keep their native instances. The independent corrected native
control matches all 252 original widths; rejected variants remain archived.

Local primary faces now retain the requested CSS weight in distinct style
FontIds, even when several weights select the same fixed face. Variable
fallbacks receive that weight on their actual native `wght` axis, preserving
other axes; primary variable faces receive it as well. System UI handling stays
on its existing path. Native CoreText variation reconstruction, rather than a
pixel-width adjustment, reproduces the intermediate-weight Chinese advances.
The independent original Courier/Arial/System spans provide 72 single-script
16px checks. The first candidate passed in the command sandbox but retained 24
differences under the native application's execution permissions: CoreText chose
a static downloadable collection there. A public family lookup supplies a
variable collection; the correction rebuilds the same PostScript face with its
original descriptor attributes and that returned source URL. Family and exact
face identity are checked, and no font path is hard-coded. All 72 widths and the
eight existing Apple text tests pass with the corrected native-permission run.
The negative results are retained in `outputs/regressions/local-css-fallback-permissions/`.
This does not accept painted glyphs or whole application/native parity; actual
GPUI scene comparisons are separate.
Source spans and native controls are retained under `status-font-synthesis-macos`
and the `status-font-variation` diagnostic reports in `outputs/`.

An earlier candidate disabled CoreText optical tracking to match Chromium,
using the setting documented in Skia's source:
https://skia.googlesource.com/skia.git/+/d2b9e48baf1697760afc1dc8ea3ad40110b8cacc/src/utils/mac/SkCTFontCreateExactCopy.cpp

That candidate was rejected after independent WKWebView and original Monaco
measurements: at 14px, macOS WKWebView uses 19px while Chromium uses 18px. The
desktop target retains CoreText tracking. No font-size or fixed-pixel correction
is applied. See `outputs/font-scroll-chromium-candidate-check.json` and the current
font source report for scope and evidence; full bitmap equality is not claimed.
