# Update VATSIM AIRAC from AIXM tool

This simple GUI tool allows updating VATSIM `.sct` files
from DFS open AIXM data.

It relies on [vatsim-parser](https://github.com/blip-radar/vatsim-parser) to
parse and render the `.sct` files, and
[aixm-rs](https://github.com/blip-radar/aixm-rs) to parse the AIXM data.

All `.sct` files in the selected folder are updated and a backup of the
existing file is written to the same folder.

The AIXM data is fetched for the current AIRAC of the
[DFS dataset releases](https://aip.dfs.de/datasets/).
