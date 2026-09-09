# mtgtuibuilder

Terminal Magic: the Gathering deck builder — Archidekt-style categories,
Commander legality, and Cardmarket prices. Rust + Ratatui.

```
┌ Deck — 34 cards ──────────────────────────┐┌ Stats ──────────────────┐
│Commander (1)                              ││Atraxa, Praetors' Voice  │
│  1x Atraxa, Praetors' Voice  €15.41       ││                     BGUW│
│Draw (3)                                   ││Price  €101.26           │
│  1x Rhystic Study  €34.69                 ││Lands  19  Avg CMC  2.87 │
│  1x Sylvan Library  €23.48                ││Pips   W4 U3 B5 G7       │
│Ramp (5)                                   ││Curve                    │
│  1x Sol Ring  €0.76                       ││ 1 █████ 2               │
│  1x Arcane Signet  €0.37                  ││ 3 ██████████████████ 7  │
└───────────────────────────────────────────┘└─────────────────────────┘
```

## Why the prices work without a Cardmarket account

Cardmarket's own API requires OAuth1 with a registered dedicated app and gates
its price guide. Scryfall redistributes the same numbers in its `eur` field,
with no auth and no rate limit — so that is where prices come from here.

Prices are **per printing**, because that is how Cardmarket works: a card
defaults to its cheapest printing, and you can pin a specific one per card.

## Install

```sh
cargo build --release
install -Dm755 target/release/mtgtuibuilder ~/.local/bin/mtgtuibuilder
```

First run downloads Scryfall's `default_cards` bulk file (~80 MB compressed),
trims it to the fields this tool uses, and caches ~19 MB to
`~/.cache/mtgtuibuilder/`. Later runs start straight from that cache.

```sh
mtgtuibuilder                  # scratch deck
mtgtuibuilder decks/atraxa.txt # open or create
mtgtuibuilder --refresh        # re-download; prices update daily upstream
```

## Keys

| Key | |
|---|---|
| `j` / `k` | move |
| `g` / `G` | top / bottom |
| `Tab` | switch deck ⇄ maybeboard |
| `/` or `a` | search and add a card |
| `+` / `-` | change quantity |
| `d` | remove card |
| `m` | move to the other board |
| `c` | set category |
| `C` | set as commander |
| `p` | choose printing (`0` resets to cheapest) |
| `i` | toggle card image |
| `s` / `S` | save / save as |
| `?` | help |
| `q` | quit |

## Card images

`i` toggles a preview of the selected card, using the pinned printing so the
art matches the version being priced. Images render through kitty's graphics
protocol, sixel or iTerm2 where available, and fall back to unicode
half-blocks everywhere else.

The preview takes its own column on terminals at least 120 columns wide;
below that it replaces the stats pane. Images are fetched on a background
thread and cached under `~/.cache/mtgtuibuilder/images/`, so scrolling never
blocks on the network and a card is only ever downloaded once.

Set `MTGTUI_IMAGE_PROTOCOL=kitty|sixel|iterm2|halfblocks` to override protocol
detection, and `--doctor` to report what the preview will actually do in the
current terminal.

**Under tmux the default is halfblocks**, because kitty's graphics protocol
does not survive it. The image travels as a passthrough sequence that tmux
drops outright when the pane is not visible, and which otherwise reaches kitty
without ever producing a placement -- confirmed by screenshotting a real
session: correct geometry on the wire, placeholders recognised, empty pane, and
`q=2` in the transmission suppressing any error kitty might have reported.
Halfblocks are ordinary coloured cells, so tmux cannot drop them. Outside tmux
kitty is used normally, and `MTGTUI_IMAGE_PROTOCOL=kitty` forces it anywhere.

### Inside tmux

Two tmux details matter, both handled automatically:

- Font size. The window-size ioctl reports no pixel dimensions under tmux, so
  the cell size is read from tmux's own `client_cell_width/height`. This is not
  cosmetic: kitty's unicode placeholders size the image to a cell grid derived
  from it, and a wrong value puts the image outside the cells meant to show it.
- Re-transmission. With `allow-passthrough on` (the common setting) tmux
  discards graphics from a pane that is not currently visible, and the image is
  otherwise transmitted only once. The app requests focus reporting and
  re-transmits when the pane comes back into view. This only matters when
  kitty is forced, since the tmux default is halfblocks.

## Deck format

Plain text in Archidekt's dialect, so decks round-trip with Archidekt, Moxfield
and MTGO rather than living in a private format:

```
1x Atraxa, Praetors' Voice (one) 196 [Commander]
1x Sol Ring (ltc) 292 [Ramp]

Maybeboard
1x Mana Crypt (2xm) 270 [Ramp]
```

Quantity may be `1` or `1x`. The printing and category are both optional, so a
bare Moxfield list loads unchanged. The maybeboard is excluded from legality
checks and from the deck price.

## Commander checks

Card count, singleton (basic lands and *Relentless Rats*-style cards exempt),
colour identity against the commander, commander eligibility, and ban list.

## Tests

```sh
cargo test
```

Tests that need the card cache skip themselves when it is absent, so
`cargo test` passes on a clean checkout. `SHOW_UI=1 cargo test
renders_a_real_commander_deck -- --nocapture` prints a rendered frame.
