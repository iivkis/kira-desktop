# Kira Calendar · Offline Sky Atlas

## Scope

The date picker remains a **small 420px popup**; the sky atlas is mounted
**only after Calendar is promoted to an ordinary Niri window** (1040×720
default, resizable). It is a 2D azimuthal equidistant chart of the sky above
the observer's horizon. The same selected calendar date is passed directly
to the atlas, so date selection immediately changes the depicted sky.
Changing date, time or latitude updates the same canvas; GSAP interpolates
sidereal rotation (shortest circular path), cancellable by any new update.
Reduced motion draws the new geometry immediately. There is no continuous
animation loop, remote fetch, browser geolocation prompt or external CDN.

Observer coordinates are **explicitly editable**. First opening uses a
**clearly labeled Greenwich example** at latitude +51.4779°, longitude 0°,
*not the user's inferred location*. Longitude east is positive, west
negative. The selected time is interpreted in the **operating system's
local time zone**, not a guessed time zone from coordinates. This makes the
instant reproducible but means 22:00 on the controls is *not* necessarily
22:00 on the Greenwich meridian. No persistent location configuration or
unapproved Kira Core write API is introduced.

The supported year interval is **1900–2100**. Beyond it, or for invalid
coordinates and time, the atlas displays an explicit unavailable message
rather than pretending to render valid astronomy. The charts are
illustrative finder charts, **not precision astrometry**: stars use
J2000 catalogue positions without individual proper motion, precession,
nutation, refraction, extinction or twilight. Constellation lines are
conventional illustrated **stick figures**, *not official IAU boundaries*.
These are fixed relative to the catalogue; the apparent sky changes with
Earth's rotation, date and the supplied observing location. The map does
not model moving Moon/planets or light pollution.

## Trusted sources and offline transformation

- Star coordinates and visual magnitude: **XHIP: An Extended Hipparcos
  Compilation**, Anderson E. & Francis C. (2012), VizieR **V/137D**,
  redistributed as `stars.6.json` by
  [d3-celestial](https://github.com/ofrohn/d3-celestial). Filter Vmag
  ≤ 5.35, retain equatorial J2000 RA/Dec and round to 0.001°.
- Conventional constellation linework:
  [d3-celestial `constellations.lines.json`](https://github.com/ofrohn/d3-celestial),
  with Russian constellation names and label positions from its
  `constellations.json`. Named bright stars additionally come from its
  `starnames.json` keyed by Hipparcos ID.
- **Pinned upstream source revision:**
  [`7e720a3de062059d4c5400a379146a601d9010e0`](https://github.com/ofrohn/d3-celestial/tree/7e720a3de062059d4c5400a379146a601d9010e0/data).
  All reduced data is bundled statically as
  `frontend/src/lib/astronomy/sky-catalog.ts`; runtime never contacts
  GitHub or any astronomical API.
- Mean Greenwich sidereal time polynomial and horizontal transform follow
  the standard astronomical hour-angle formulas, cf.
  [U.S. Naval Observatory / Astronomical Almanac](https://aa.usno.navy.mil/faq/GAST).
  Fixed-star calculations are implemented in
  `frontend/src/lib/astronomy/sky-model.ts` (no new npm dependencies).
- Official status and names of 88 constellation regions:
  [International Astronomical Union (IAU)](https://www.iau.org/IAU/Astronomy-FAQs/Constellations.aspx).
  The IAU standardizes constellation **areas/boundaries**; the illustrated
  connecting segments are *not* those boundaries. The source GeoJSON has
  89 figure records because Serpens appears in two regions.

## Testing

`frontend/src/lib/astronomy/sky-model.test.ts` verifies catalogue
bounds, coordinate validation, Greenwich sidereal angle at J2000,
zenith/horizon cardinal projection, date/time and shortest-path wrap.
The Python calendar contract checks that the atlas is gated on the
generation-matched detached-window event and does not request geolocation
or runtime network access. Existing overlay smoke tests still validate
the lightweight calendar popup. For actual visual QA, start nested Niri,
open Calendar, select a date, promote to an ordinary window and verify:
the star field renders; changing date/time rotates it; changing
latitude/longitude changes visible constellations; closing the window
unmounts and releases the canvas; reduced-motion and repeated input changes
leave no ongoing tween.

## Third-party BSD-3-Clause notice (d3-celestial catalogue reduction)

The following license applies to the upstream d3-celestial material
reduced and included in `sky-catalog.ts`:

```text
Copyright (c) 2015, Olaf Frohn
All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```
