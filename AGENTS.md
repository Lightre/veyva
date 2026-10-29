# Project instructions

- Keep all code, identifiers, UI text, documentation, logs and commit messages in English.
- Prefer the smallest reasonable change and targeted verification.
- Keep capture/encode work independent of the settings UI. Avoid continuous redraw while idle.
- Treat resource budgets and benchmark targets as goals until measured on identified hardware.
- Bound frame queues, encoded history and pending exports; never let a slow disk block capture indefinitely.
- Preserve decodable keyframe boundaries and audio/video timestamps when evicting history.
- Reuse a capture/encode session for replay and recording where settings match.
- Do not silently enable expensive software encoding or recording at startup.
- Preserve required upstream notices with any copied or adapted code; record the exact source revision.
- Never claim a recording feature works based only on compilation.
- No accounts, telemetry, cloud uploads, overlays, gallery or updater in the first milestone.
