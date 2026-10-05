# Third-party notices

## pixel-ssh

The clean browser terminal (`web/play/`, through `crates/nethacked-wasm/src/pixel.rs`) renders with crates from [pixel-ssh](https://github.com/dmytro-yemelianov/pixel-ssh): `pixel-ssh-framebuffer`, `pixel-ssh-view` and `pixel-ssh-render-web`. pixel-ssh is MIT licensed.

## Bitmap fonts

The pixel renderer includes bitmap fonts from VileR's [Oldschool PC Font Pack v2.2](https://int10h.org/oldschool-pc-fonts/), under [Creative Commons Attribution-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-sa/4.0/):
- the CP437 tables, through [Susam Pal's pcface](https://github.com/susam/pcface);
- the Cyrillic tables, from the pack's BmPlus fonts.

pixel-ssh's `crates/framebuffer/assets/README.md` records the exact sources, revisions and changes for every table. The fonts are shared under CC BY-SA 4.0, and the rest of NetHackED keeps its own license (NGPL).
