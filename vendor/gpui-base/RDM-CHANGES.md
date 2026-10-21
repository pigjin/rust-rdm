# Local desktop additions

This is gpui-base 0.7.3, with its Apache-2.0 license retained. The desktop client
adds selectable caret shapes, separate placeholder colors for light and dark themes, text dragging as one undo transaction, and
modifier-click URL navigation so Tiny RDM editor preferences affect native
editing. The unused gpui-fast test dependencies are omitted; the application uses the
standard GPUI platform. The popup host also supports an optional virtual horizontal anchor and physical-pixel
positioning for the desktop footer menus. Existing callers retain
whole-logical-pixel offset rounding.

The stateful Popover forwards the existing Popup virtual horizontal anchor,
physical-pixel alignment and preserved out-of-viewport anchor options. These
are opt-in; defaults stay unchanged. The client tooltip uses them with the
original fractional grid/inline layout while retaining Popover dismissal,
focus restoration and deferred-context cleanup.

`InputEditorStyle::gutter` optionally customizes line-number digits and spacing,
folding width, active line-number color, gutter-only line decoration, and fold
icons shown on hover. It supports the original JSON editor's two-digit gutter
without changing existing editors' three-digit geometry and line highlighting.

Application text decorations compose in input order, after syntax styles. The
generic GPUI combiner folds an unordered set and could let an arbitrary syntax
token override an application color. A regression covers multiple independent
tokens; the ordered sweep retains the collection precedence contract.

The opt-in `test-support` module can retain active selection painting for an
offscreen snapshot using a scoped guard. It paints the actual focus and range
without activating an OS window, and does not affect ordinary builds.

Interactive hover cards keep hit-testing pointer position after keyboard input.
Typing into a field inside the card no longer dismisses it solely because the
input modality changed. A regression also checks that moving out still closes it.

`SelectableText` has opt-in policies for recycled content: selected endpoints
can move to the new text's start when a retained row changes, and copied blocks
can preserve empty or whitespace fragments. CSV virtual rows use these to match
the original DOM's selection and trailing line separator after scrolling.
Existing callers retain their previous endpoint and copy policies. A regression
checks both the default filtering and explicitly preserved blank blocks.

`Positioner` and `Popup` expose an opt-in horizontal anchor preservation policy.
The collection editor uses it to match the original dropdown when a long
trigger extends outside the viewport. Vertical flipping/clamping and default
callers remain unchanged. Three geometry regressions cover overflow, vertical
flipping and unchanged in-view placement.

`InputEditorStyle` optionally supplies an overview ruler and a monochrome SVG
tile for diagnostic underlines. The ruler maps ranges and cursor positions
through the wrap/fold display map, merges overlapping marks of the same color,
and uses its own diagnostic palette. The SVG mode clips tiled underlines to
visible wrapped fragments and suppresses the platform wavy underline. Both
options default to absent. The application supplies the Monaco SVG and colors;
the base crate contains no application-specific artwork. Application QA covers
six syntax/warning render cases; exact glyph and antialiasing equality is not
claimed, and the Emoji sample retains a documented extent difference.

Editors can opt into `DiagnosticPresentation` callbacks for grouped hover
rendering, marker navigation, dismissal and diagnostic updates. The host supplies
hover delays and content; pointer capture keeps a popup open while the pointer
enters it, and ordinary blur, editing, scrolling or Escape cancel its pending
state. Moving focus to selectable text inside the host hover preserves that
surface; a regression separately checks ordinary blur still dismisses it.
Alt+F8 and Alt+Shift+F8 apply only to editor-mode inputs. The application owns
the Monaco presentation, colors and artwork; other callers keep the default
diagnostic popup. A controlled-clock regression checks blur cancellation and
reopening the same marker after its cached group is cleared.

An optional `EditorViewZone` reserves vertical space after a tracked byte
position without inserting text or adding an undo transaction. Display/wrap
geometry, hit testing, selection, caret, gutter, diagnostic marks and scroll
extent account for this space. Its regression covers following-line placement,
hit testing, wrapping and edits through undo/redo. The diagnostic host also reads
the actual shaped editor font after prepaint instead of guessing its metrics.

`TextSelectionSnapshot::project_runs` exposes the existing visual-run projection
without mutating selection. The application maps shaped diagnostic fragments
back to their source byte ranges so soft wrapping and inline padding do not add
copied characters, while hard newlines and tabs survive. Selection remains in
the window selection system; the application supplies its diagnostic focus,
copy and navigation handlers.
