# JDmusings

JDmusings records audio from your microphone, transcribes it locally with
[whisper.cpp](https://github.com/ggml-org/whisper.cpp), and creates a note
or to-do in [Joplin](https://joplinapp.org) through the Web Clipper API.
Everything runs locally — no audio is ever sent to a cloud service.

## Features

- CLI (`jdmusings`) and GTK4 GUI (`jdmusings-gui`)
- Transcription via whisper.cpp with `--no-fallback` for fewer false words
- Hallucination-token filtering (`[BLANK_AUDIO]`, `[ Silence ]`, etc.)
- Creates regular notes **or** Joplin to-dos with an optional due date
- Due date stored in Joplin metadata **and** written visibly in the note body
- Notebook picker, calendar date/time selector, and status feedback in GUI

## Requirements

- Linux with ALSA (`arecord` from `alsa-utils`)
- [Joplin desktop](https://joplinapp.org) with Web Clipper enabled
- [whisper.cpp](https://github.com/ggml-org/whisper.cpp) built locally with at
  least one model downloaded
- GTK4 runtime libraries (GUI only)

> **Building from source** also requires the Rust toolchain and GTK4 development
> headers — see [Build from source](#build-from-source).

## Quick start

### 1 — Install system dependencies

**Fedora:**
```bash
sudo dnf install -y alsa-utils gtk4 curl git cmake gcc-c++ make
```

**Debian / Ubuntu:**
```bash
sudo apt install -y alsa-utils libgtk-4-1 curl git cmake g++ make
```

### 2 — Install Joplin and enable Web Clipper

```bash
curl -s https://raw.githubusercontent.com/laurent22/joplin/dev/Joplin_install_and_update.sh | bash
```

Launch Joplin, then go to **Tools → Options → Web Clipper** and enable the
service. JDmusings reads the token automatically from
`~/.config/joplin-desktop/settings.json`, or you can set `JOPLIN_TOKEN`.

### 3 — Build whisper.cpp

```bash
git clone https://github.com/ggml-org/whisper.cpp.git ~/whisper.cpp
cmake -S ~/whisper.cpp -B ~/whisper.cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build ~/whisper.cpp/build -j$(nproc)
bash ~/whisper.cpp/models/download-ggml-model.sh base.en
```

For better accuracy use `small.en` or `medium.en` and set `WHISPER_MODEL`.

### 4 — Get JDmusings binaries

#### Option A — Download from GitHub Releases (recommended)

Visit the [latest release](https://github.com/NormG/Joplin-dictate/releases/latest)
and download `jdmusings` and `jdmusings-gui`, then:

```bash
chmod +x jdmusings jdmusings-gui
mv jdmusings jdmusings-gui ~/.local/bin/
```

#### Option B — Build from source

```bash
git clone git@github.com:NormG/Joplin-dictate.git ~/Projects/joplin-dictate
```

Install Rust build dependencies:

**Fedora:**
```bash
sudo dnf install -y rust cargo rustfmt clippy gtk4-devel
```

**Debian / Ubuntu:**
```bash
sudo apt install -y rustc cargo rustfmt clippy libgtk-4-dev
```

Build:
```bash
cargo build --release --bins
# Binaries land at:
#   target/release/jdmusings
#   target/release/jdmusings-gui
```

## CLI usage

Record and create a note (title derived from transcript):
```bash
jdmusings
```

Custom title:
```bash
jdmusings --title "Meeting notes"
```

Create a to-do:
```bash
jdmusings --todo
```

Create a to-do with a due date:
```bash
jdmusings --todo --due "2026-09-10 16:30"
```

Create a note in a specific notebook (use the notebook's Joplin folder ID):
```bash
jdmusings --parent FOLDER_ID
```

Combine flags — titled to-do with due date in a specific notebook:
```bash
jdmusings --title "Submit report" --todo --due "2026-09-15 09:00" --parent FOLDER_ID
```

Smoke-test using an existing WAV file (no microphone needed):
```bash
jdmusings --title "Test note" --audio-file ~/whisper.cpp/samples/jfk.wav
```

Full flag reference:
```
Options:
  -p, --parent <PARENT_ID>       Joplin notebook/folder ID
  -t, --title <TITLE>            Custom note title
  -d, --todo                     Create a to-do instead of a regular note
  -D, --due <DUE>                Due date in "YYYY-MM-DD HH:MM" format
      --audio-file <AUDIO_FILE>  Use an existing WAV instead of recording
  -h, --help                     Print help
  -V, --version                  Print version
```

## GUI usage

```bash
jdmusings-gui
```

### Workflow

1. Start Joplin and confirm Web Clipper is enabled.
2. Launch `jdmusings-gui` and wait for **Ready.** in the status bar.
3. Pick a notebook from the dropdown.
4. (Optional) Enter a custom title.
5. (Optional) Check **To-do** and choose a due date with the calendar picker.
6. Click **Start Recording** (button turns blue).
7. Speak clearly for at least 2–3 seconds.
8. Click **Stop Recording** (button is red while recording).
9. The status bar shows *Transcribing…* then *Note created* on success.
10. Check Joplin for the new note or to-do.

## GNOME launcher

Run once to install local wrappers and a desktop launcher:

```bash
REPO="$HOME/Projects/joplin-dictate"

mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps

cp "$REPO/jdmusings.svg" ~/.local/share/icons/hicolor/scalable/apps/

cat > ~/.local/bin/jdmusings <<SH
#!/usr/bin/env bash
exec "$REPO/target/release/jdmusings" "\$@"
SH
chmod +x ~/.local/bin/jdmusings

cat > ~/.local/bin/jdmusings-gui <<SH
#!/usr/bin/env bash
exec "$REPO/target/release/jdmusings-gui" "\$@"
SH
chmod +x ~/.local/bin/jdmusings-gui

cat > ~/.local/share/applications/jdmusings.desktop <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=JDmusings
GenericName=Voice Note Recorder
Comment=Record voice notes and send them straight to Joplin
Exec=$HOME/.local/bin/jdmusings-gui
Icon=jdmusings
Terminal=false
Categories=AudioVideo;Utility;
Keywords=joplin;note;dictate;voice;microphone;record;transcribe;whisper;todo;
StartupNotify=true
EOF

update-desktop-database ~/.local/share/applications/
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/ || true
```

Search for **JDmusings** in GNOME Activities to launch the GUI.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `JOPLIN_TOKEN` | *(auto-read from settings.json)* | Web Clipper auth token |
| `JOPLIN_HOST` | `http://127.0.0.1:41184` | Web Clipper base URL |
| `WHISPER_DIR` | `~/whisper.cpp` | whisper.cpp checkout root |
| `WHISPER_MODEL` | `$WHISPER_DIR/models/ggml-base.en.bin` | Model file path |

## How it works

1. `arecord` records mono 16 kHz PCM audio until Stop is pressed (or EOF).
2. `whisper-cli` transcribes the WAV file using `--no-fallback`.
3. Known hallucination tokens are stripped:
   `[Blank Audio]`, `[BLANK_AUDIO]`, `[ Silence ]`, `[noise]`, `[Music]`.
4. If no speech text remains, JDmusings exits without creating a note.
5. The first sentence of the transcript becomes the note title (unless
   `--title` is given).
6. If a due date is set, it is stored in Joplin's `todo_due` metadata field
   **and** prepended to the note body as a human-readable `Due: ...` line.
7. A JSON payload is `POST`ed to the Joplin Web Clipper `/notes` endpoint.

## Validation

```bash
cargo fmt --check
cargo test
cargo clippy --bins -- -D warnings
cargo build --release --bins
```

## Known limitations

- Background audio is transcribed — music, TV, or nearby conversation will
  appear in the note.
- Very short or quiet recordings can still produce short-word hallucinations
  such as `you` or `the`.
- The default `base.en` model is English-only. Use a multilingual model for
  other languages.
- Transcription uses the CPU by default; GPU acceleration requires a custom
  whisper.cpp build.

## License

MIT.
