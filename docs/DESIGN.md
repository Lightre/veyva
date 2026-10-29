# Desktop design

Editable Figma reference: https://www.figma.com/design/TRYZOThKW4DUNH4Lb5vWPG?node-id=6-129

The design separates Normal Recording (Start, Pause/Resume, Finish & Save) from Instant Replay (armed history, Save Replay, configurable shortcut and independent enable/disable). Replay starts at application launch by default. Normal recording pauses do not pause replay.

Design assets use imported Simple Design System button instances and body typography, with Veyva ink/mint colors and a restrained desktop layout. The Figma file contains editable text/frames and reusable button instances, not a flattened screen image. The recording and replay panel design contexts and screenshots were inspected. The native egui implementation translates the layout and controls to the existing renderer; runtime pixel matching has not been verified and native font rendering differs from Figma's Inter reference.

System audio state and failures appear explicitly. Preferences include startup replay, audio capture, capture quality/history/budget, and a modifier + F1-F12 shortcut picker. The default shortcut is Alt+F1; conflicts produce a visible registration error. Output location is currently kept for the application session.
