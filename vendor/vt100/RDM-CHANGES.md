# Tiny RDM compatibility changes

Based on vt100 0.16.2 (MIT). Cell width follows the original xterm.js Unicode 6 provider rather than the current Unicode width tables. `src/width.rs` ports the original xterm.js tables and rules; its MIT license is included in `XTERM-LICENSE`.
