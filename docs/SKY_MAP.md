# Kira Calendar · Offline Sky Atlas

## Scope

The native date picker remains a compact ~420px transient popup; the star
atlas is **shown only when Calendar is detached** into a regular Niri-managed
xdg-toplevel (1040×720 default, resizable). The same retained GTK date rail
and SkyMap widget share one FlowBox that wraps vertically on narrow windows.
Actual wrap/scroll/focus at 680px still require nested Niri verification.
The canonical palette remains Black, Prussian Blue, Orange, Grey, White.
The Cairo chart uses a horizon-clipped azimuthal equidistant projection.
It renders J2000 fixed stars and conventional illustrated constellations
using the *same selected calendar day* as the compact date picker.
A native GTK frame-clock token interpolates sidereal time and latitude in
420ms over the shortest sidereal arc. New input cancels the old animation;
reduced-motion settles instantly. No continuous animation loop, network,
browser geolocation permission or external CDN exists. The former Canvas
backing-buffer limit of 1536 physical pixels is **not yet verified** for
the new GtkDrawingArea and remains an explicit scale/performance acceptance gate.

The controls now use a wrapping native FlowBox so manual coordinate/time
editors and actions remain available in a narrow detached window. **−1 ч**
and **+1 ч** adjust the host-local observation clock without silently moving
the Calendar's selected date; nudges are disabled if the clock is invalid or
would cross midnight. **СБРОС** restores Greenwich latitude, longitude
**and 22:00**, but intentionally retains the selected Calendar day.
Each invalid coordinate/time field receives a visible orange border and a
specific Russian diagnostic. Unsupported dates and nonexistent local DST
clock instants are rejected; the renderer is hidden rather than showing
an invented sky. The chart's accessible description includes the actual
manual coordinates and selected date/time. Model tests cover boundary
nudges and validation; nested-Niri wrapping/focus remains unverified.

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
  `crates/kira-ui/src/sky_catalog.rs`; runtime never contacts
  GitHub or any astronomical API.
- Mean Greenwich sidereal time polynomial and horizontal transform follow
  the standard astronomical hour-angle formulas, cf.
  [U.S. Naval Observatory / Astronomical Almanac](https://aa.usno.navy.mil/faq/GAST).
  Fixed-star calculations are implemented in
  `crates/kira-ui/src/sky_model.rs` (no runtime astronomy packages).
- Official status and names of 88 constellation regions:
  [International Astronomical Union (IAU)](https://www.iau.org/IAU/Astronomy-FAQs/Constellations.aspx).
  The IAU standardizes constellation **areas/boundaries**; the illustrated
  connecting segments are *not* those boundaries. The source GeoJSON has
  89 figure records because Serpens appears in two regions.

## Native testing and remaining acceptance

`crates/kira-ui/src/sky_model.rs` contains independent tests for:
observer validation, local wall-clock date and 1900–2100 bounds,
J2000 Greenwich sidereal angle, zenith, eastern/western horizon,
cardinal sign and shortest circular angle. `sky_catalog.rs` verifies exact
pinned catalogue population and astronomical bounds. These tests are run
in `task shell:check` / GitHub CI.

The actual Cairo renderer and detached GtkFlowBox must still be exercised
in *isolated* nested Niri, never by replacing the host's compositor.
Check 1100px and 680px Calendar widths, selected-date updates, explicit
coordinates and local time, zero network/geolocation access, invalid-state
message, interruption/cancellation after rapid edits, reduced motion,
altitude clipping, focus, and 1536px-equivalent high-DPI memory budget.
Paired source/native screenshot and motion-frame evidence are not yet
recorded. This is an implemented native candidate, not certified parity.

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

## Native migration status

Native `kira-ui::SkyMap` now draws the catalogue with Cairo, follows the
retained Calendar's selected date and exposes the explicit location/time
controls in detached mode. Headless data/model tests and compile checks do
**not** verify visual fidelity, animation timelines, monitor scaling,
physical pixel-budget equivalence or accessibility.
