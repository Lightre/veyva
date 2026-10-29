# Brand direction

Working name: **Veyra Replay**. Repository slug: `veyra`.
Tagline: **Keep the moment. Keep your frames.**

The name is short and easy to place in a tray menu. It is provisional: an existing video intelligence product uses Veyra (https://www.veyra.fi/). Do not purchase domains or publish releases under this name before selecting and checking a final brand. A web search does not establish trademark availability.

## Visual identity

- Background: ink `#10151C`.
- Surface: slate `#1A2330`.
- Primary: mint `#70E2BC`.
- Text: mist `#EEF4F7`; secondary text `#A9B7C6`.
- Warning: amber `#FFC46B`; failure: coral `#FF7D87`.
- Typography: system Segoe UI; no downloaded fonts.
- Mark: an open replay arc enclosing a play triangle. See `assets/mark.svg`; the Windows icon is `assets/veyra.ico`.

## Interface template

One compact native control window: title, capture status, Low End profile, replay duration, output folder, and primary actions. Initial state: "Replay off". Future actions: "Start replay", "Save last 30 seconds", "Start recording", "Settings". Disable saving until a decodable history exists. Always show actual buffered duration when shorter than requested.

Tray menu: "Open Veyra", "Start replay" / "Stop replay", "Save replay", "Quit". Proposed save shortcut: Alt+Shift+S, configurable and checked for conflicts.

Use calm factual language. Never display fake CPU usage, FPS impact, saved clips or encoder availability. No dashboard charts, autoplay preview or decorative animation in the initial control window.

The application UI has not been implemented in this scaffold.

## Asset generation

`assets/theme.css` exposes the palette and system font as reusable tokens. Regenerate the Windows icon from the mark geometry with `python tools/generate_icon.py` (requires Pillow; no network access).
