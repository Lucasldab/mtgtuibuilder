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
| `s` / `S` | save / save as |
| `?` | help |
| `q` | quit |

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
