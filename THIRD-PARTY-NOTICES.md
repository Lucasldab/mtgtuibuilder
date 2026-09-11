# Third-party notices

## ratatui-image

Card previews are drawn by
[ratatui-image](https://github.com/ratatui/ratatui-image), whose code is linked
into this binary. This project carried its own kitty placeholder emission for a
while; that work was contributed upstream as
[ratatui-image#201](https://github.com/ratatui/ratatui-image/pull/201) and the
local copy has since been removed, so nothing here is derived from it any more.

ratatui-image is distributed under the MIT licence:

```
The MIT License (MIT)

Copyright (c) 2022 Atanas Yankov (code and inspiration taken from https://github.com/atanunq/viuer)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Card data and images

Card data, Cardmarket prices and card images come from
[Scryfall](https://scryfall.com), which permits this use and asks that clients
cache rather than re-fetch — this project does. Card names, images and text are
copyright Wizards of the Coast; this project is unaffiliated with Wizards of the
Coast and with Scryfall.

Deck suggestions come from [EDHREC](https://edhrec.com), read from the JSON
backing its own pages. EDHREC publishes no documented API, so that shape is
unofficial and may change; each commander is fetched at most once and cached.
This project is unaffiliated with EDHREC.
