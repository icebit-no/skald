# Icon files

The app icon is ᚨ cut as a V-groove into light granite and painted red ochre; the menu bar icon is the same
rune flat, in one colour. What they look like and why is in [design.md](../../docs/design.md#identity) and
[D-17](../../docs/decisions.md). This folder holds the PNG files and the script that renders them.

| File | Pixels | Drawing |
|---|---|---|
| `app-icon-1024.png`, `-512`, `-256`, `-128`, `-64` | as named | The chosen granite drawing |
| `app-icon-32.png`, `app-icon-16.png` | as named | The same stone and rune, drawn again for the size |
| `menu-bar-icon.png`, `menu-bar-icon@2x.png` | 18, 36 | The flat rune on an 18 pt square, as a macOS template image |

## Render them again

```sh
python3 assets/icon/render.py
```

`render.py` needs Python 3 and nothing else: it uses the standard library only. It takes about ten seconds and
writes the PNG files next to itself. The stone comes from seeded noise, so every run writes the same pixels.
To check a change, render to another folder with `--out DIR` and compare.

## How each size is made

- **1024 to 64 px.** The stone is a height map (face, groove, paint) lit from the top left. It is rendered
  once at 2048 px and halved down, so every file is the mean of 2 × 2 blocks of the one above. Rendering each
  size on its own was tried: from 256 px down the grain is finer than a pixel and comes out as salt-and-pepper
  noise.
- **32 and 16 px.** Not downscales. The stone's sides and the stave sit on pixel edges, the groove is a
  quarter of the stone wide (4 px and 2 px) against a ninth in the large drawing, and the twigs are moved apart
  so stone shows between them. The cut is steady, without the large drawing's wobble. The 32 px stone has
  half the grain, with grains about a pixel wide; the 16 px stone has none, only the mottling.
- **Groove ends.** At every size the groove ends flat at a free end, and the twigs start butt on the stave's
  centre line.
- **Menu bar.** Three filled strokes, 2 pt wide, black with alpha. The stave has square ends; the twigs start
  butt on the stave's centre line and end square. The stave lies on whole points, so it is sharp at 1x and 2x.

The stone, the red, the light and the rune's proportions at the large sizes are settled in D-17. The numbers
for them are in the section «The drawing» of `render.py`; changing them is a decision, not a tweak.

Packing these into an `.icns` and wiring the bundle is part of the `.app` bundle item in
[plan.md](../../docs/plan.md).
