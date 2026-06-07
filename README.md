# JDmusings
JDmusings records audio from your microphone, transcribes it locally with [whisper.cpp](https://github.com/ggml-org/whisper.cpp), and creates a note or to-do in [Joplin](https://joplinapp.org) through the Web Clipper API.
Everything runs locally. No audio is sent to a cloud service.
## Status
This branch is a Rust rewrite of the original `joplin-dictate.sh` and Python GTK GUI. The old Bash/Python entry points have been replaced by:
- `jdmusings` — CLI
- `jdmusings-gui` — GTK4 desktop GUI
## Requirements
- Linux with ALSA (`arecord` from `alsa-utils`)
- Rust toolchain (`rust`, `cargo`, `rustfmt`, `clippy`)
- GTK4 development/runtime libraries
- Joplin desktop with Web Clipper enabled
- `whisper.cpp` built locally, with at least one model downloaded
On Fedora:
```bash
sudo dnf install -y rust cargo rustfmt clippy gtk4-devel git cmake gcc-c++ make alsa-utils curl jq fuse-libs
```
On Debian/Ubuntu:
```bash
sudo apt install -y cargo rustc rustfmt clippy libgtk-4-dev git cmake g++ make alsa-utils curl jq libfuse2
```
## Install Joplin
Use the official AppImage installer:
```bash
curl -s https://raw.githubusercontent.com/laurent22/joplin/dev/Joplin_install_and_update.sh | bash
```
Launch Joplin once, then enable Web Clipper:
1. Open Joplin.
2. Go to Tools → Options → Web Clipper.
3. Enable the Web Clipper service.
4. Copy the authorization token if you want to export it manually.
JDmusings can also read the token from Joplin's own config file at `~/.config/joplin-desktop/settings.json`.
## Build whisper.cpp
```bash
git clone https://github.com/ggml-org/whisper.cpp.git ~/whisper.cpp
cmake -S ~/whisper.cpp -B ~/whisper.cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build ~/whisper.cpp/build -j$(nproc)
bash ~/whisper.cpp/models/download-ggml-model.sh base.en
```
For better accuracy, use a larger model such as `small.en` or `medium.en` and set `WHISPER_MODEL`.
## Build JDmusings
From this repository:
```bash
cargo build --release --bins
```
The release binaries are:
- `target/release/jdmusings`
- `target/release/jdmusings-gui`
## CLI usage
Create a normal note:
```bash
target/release/jdmusings
```
Create a note with a custom title:
```bash
target/release/jdmusings --title "Meeting notes"
```
Create a to-do:
```bash
target/release/jdmusings --todo
```
Create a to-do with a due date:
```bash
target/release/jdmusings --due "2026-09-10 16:30"
```
Create a note in a specific notebook/folder:
```bash
target/release/jdmusings --parent FOLDER_ID
```
Deterministic test using an existing WAV file:
```bash
target/release/jdmusings --title "Test note" --audio-file ~/whisper.cpp/samples/jfk.wav
```
## GUI usage
Run:
```bash
target/release/jdmusings-gui
```
The GUI provides:
- notebook picker
- optional title field
- to-do checkbox
- calendar due-date picker with hour/minute spinners
- blue Start Recording button
- red Stop Recording button while recording
- status messages for dependency checks, recording, transcription, and note creation
Workflow:
1. Start Joplin and ensure Web Clipper is enabled.
2. Launch `jdmusings-gui`.
3. Wait for `Ready.`.
4. Choose notebook/title/to-do/due date as needed.
5. Click Start Recording.
6. Speak clearly for at least 2–3 seconds.
7. Click Stop Recording.
8. Check Joplin for the created note or to-do.
## GNOME launcher
Install local wrappers and desktop launcher:
```bash
mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
cp jdmusings.svg ~/.local/share/icons/hicolor/scalable/apps/
cat > ~/.local/bin/jdmusings <<'SH'
#!/usr/bin/env bash
exec "$HOME/Projects/joplin-dictate/target/release/jdmusings" "$@"
SH
chmod +x ~/.local/bin/jdmusings
cat > ~/.local/bin/jdmusings-gui <<'SH'
#!/usr/bin/env bash
exec "$HOME/Projects/joplin-dictate/target/release/jdmusings-gui" "$@"
SH
chmod +x ~/.local/bin/jdmusings-gui
cat > ~/.local/share/applications/jdmusings.desktop <<'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=JDmusings
GenericName=Voice Note Recorder
Comment=Record voice notes and send them straight to Joplin
Exec=/home/USER/.local/bin/jdmusings-gui
Icon=jdmusings
Terminal=false
Categories=AudioVideo;Utility;
Keywords=joplin;note;dictate;voice;microphone;record;transcribe;whisper;todo;
StartupNotify=true
EOF
sed -i "s|/home/USER/|$HOME/|g" ~/.local/share/applications/jdmusings.desktop
update-desktop-database ~/.local/share/applications/
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/ || true
```
Search for `JDmusings` in GNOME Activities.
## Environment variables
- `JOPLIN_TOKEN` — Web Clipper token. Optional if Joplin's settings file contains `api.token`.
- `JOPLIN_HOST` — default `http://127.0.0.1:41184`.
- `WHISPER_DIR` — default `~/whisper.cpp`.
- `WHISPER_MODEL` — default `$WHISPER_DIR/models/ggml-base.en.bin`.
## How it works
1. `arecord` records mono 16 kHz PCM audio.
2. `whisper-cli` transcribes the WAV using `--no-fallback`.
3. Known token hallucinations are stripped: `[Blank Audio]`, `[BLANK_AUDIO]`, `[ Silence ]`, `[noise]`, `[Music]`.
4. If no speech remains, JDmusings exits without creating a note.
5. If a due date is set, it is stored in Joplin's `todo_due` metadata and prepended to the note body as a visible `Due: ...` line.
6. A JSON payload is posted to the Joplin Web Clipper `/notes` endpoint.
## Validation
Recommended checks before committing:
```bash
cargo fmt --check
cargo test
cargo clippy --bins -- -D warnings
cargo build --release --bins
```
## Known limitations
- Background audio is transcribed. Music, TV, or nearby conversation will appear in the note.
- Very short or very quiet recordings may still produce short-word hallucinations such as `you`.
- The default `base.en` model is English-only.
- The default build uses CPU transcription.
## License
MIT.
