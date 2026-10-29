# Desktop design

Approved C references:
- [Dark desktop](https://www.figma.com/design/TRYZOThKW4DUNH4Lb5vWPG?node-id=8-19)
- [Light desktop](https://www.figma.com/design/TRYZOThKW4DUNH4Lb5vWPG?node-id=8-99)

The approved layout replaces the vertical sidebar with a custom titlebar and horizontal Capture, Preferences and About navigation. Normal recording has a large timecode and separate Start, Pause/Resume and Finish & save controls. Instant replay retains independent arming, history status and saving. The right inspector presents capture, audio, destination and appearance settings.

Dark is the default. Both themes use one semantic palette (background, surface, border, text, muted text, accent and accent text). Appearance is saved immediately without restarting capture or persisting unsaved capture preferences. Existing version 1 and 2 settings without a theme load as dark. Capture preferences retain explicit saving and require capture to stop before editing.

The custom titlebar provides a drag region, double-click maximize/restore, minimize, maximize/restore and safe close. Five-point edge and corner handles request native resizing. Native snap behavior, DPI scaling and window controls require Windows runtime verification.

Both Figma frames contain editable layout/text and imported Simple Design System button instances. Their screenshots were inspected. The native egui translation reuses the embedded Veyva icon and existing command/session interface; it does not use a screenshot as the UI. Native rendering has not been visually verified and its default font differs from Figma Inter.

The inspector reports actual capture configuration and audio state. Quality, frame rate, monitor and system audio are edited in Preferences. Recording area is currently the entire display; audio endpoints are selectable in Capture / Audio. Device selection is implemented; gain controls are not included. Microphone is optional and defaults off. Output folder remains session-local.

Background preferences default off. Close-to-tray retains the existing session; per-user Windows login startup launches `gui --background` and arms replay. The tray menu exposes Show, Save Replay and Quit. These runtime paths require manual acceptance.
