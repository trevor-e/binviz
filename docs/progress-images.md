# Decompilation progress images

Open a binary and choose **Progress** (shortcut **R**), or **Decompilation
progress treemap** on Overview. Each tile represents a function, sized by its
code bytes and colored by its recorded decompilation status. Hover for the
function name, address, byte size, status, best score and source/section group;
click a tile or focus it and press Enter to inspect its code.

Use **Export PNG** for a shareable raster image, or **Export SVG** for a scalable,
self-contained image. Choose 1600 × 900 or 2560 × 1440. The image contains its
title, totals and legend and does not need a server or external fonts. Changing
notes refreshes the treemap; export it after each milestone to retain snapshots.

Colors distinguish **Matched**, **Nonmatching C**, **In progress**, **Attempted**,
**To do**, **Skipped** and **Library**. Exact matched code bytes determine the
headline percentage; nonmatching C and partial match scores do not count as
exact matches. Library code is hidden by default and remains outside game
progress totals when **Show library code** is checked. Function counts are shown
separately, since many small matches can hide the work left in larger functions.

The map uses binviz's existing progress inventory and recorded statuses, not a
new compilation check. Functions with no status are **To do**. A sized note that
merges function fragments counts once, just as in `progress json`. Zero-sized
functions count in the summary but have no image area. Source files in notes
define groups; otherwise the containing section does. Import your project's
notes with **Import progress notes…**, or record outcomes with `match --record`, `report`, or MCP `mark` to keep
progress current. Unrecognized code/data still depends on the binary's inventory.

Generate an image from the command line without opening the browser:

```text
binviz progress game.exe --svg progress.svg
binviz progress game.exe --notes game-notes.json --svg progress.svg --width 2560 --height 1440 --include-library
```

CLI defaults are 1600 × 900, library hidden. Width supports 640–8192 and height
360–8192. `progress` automatically reads `<file>.binviz-notes.json` and its
journal when present; an explicit `--notes` takes precedence. Existing
`progress` text and `progress json` remain available.

For an open binary in MCP:

```json
{"path":"progress.svg","format":"svg","width":1600,"height":900,"include_library":false}
```

Pass this to `export_progress`; its default format remains objdiff JSON. Browser,
CLI and MCP use the same SVG renderer and progress inventory.
