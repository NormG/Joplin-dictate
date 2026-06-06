# joplin-dictate

Record audio from your microphone, transcribe it locally with
[whisper.cpp](https://github.com/ggml-org/whisper.cpp), and create a new
note (or to-do) in [Joplin](https://joplinapp.org) via its Web Clipper
HTTP API.

Everything runs on your own machine — no audio is ever sent to a cloud
service.

## Requirements

- Linux with ALSA (`arecord` from the `alsa-utils` package)
- `bash`, `curl`, `jq`, `sed`, `date` (GNU coreutils)
- A working `whisper.cpp` build, with at least one model downloaded
- The Joplin desktop app with the **Web Clipper** service enabled

### 1. System packages

On Fedora:

```bash
sudo dnf install -y git cmake gcc-c++ make alsa-utils jq curl fuse-libs
```

> `fuse-libs` is required to run the Joplin AppImage on Fedora.

On Debian / Ubuntu:

```bash
sudo apt install -y git cmake g++ make alsa-utils jq curl libfuse2
```

### 2. Install Joplin

Use the official install script (downloads the AppImage to `~/.joplin/`):

```bash
curl -s https://raw.githubusercontent.com/laurent22/joplin/dev/Joplin_install_and_update.sh | bash
```

Then launch Joplin once so it creates its configuration, and enable the
Web Clipper (see [Configuring Joplin](#configuring-joplin) below).

### 3. Build whisper.cpp and download a model

```bash
git clone https://github.com/ggml-org/whisper.cpp.git ~/whisper.cpp
cmake -S ~/whisper.cpp -B ~/whisper.cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build ~/whisper.cpp/build -j$(nproc)
bash ~/whisper.cpp/models/download-ggml-model.sh base.en   # or small.en, medium.en, etc.
```

## Configuring Joplin

1. Open Joplin → **Tools → Options → Web Clipper**.
2. Click **Enable Web Clipper Service**. The default port is `41184`.
3. Copy the **authorisation token** shown on that screen.
4. Add the following to `~/.bashrc` so the token is always available and
   stays current even if you later renew it from the Joplin UI:

   ```bash
   # Joplin Web Clipper token — read from Joplin's own config file
   export JOPLIN_TOKEN
   JOPLIN_TOKEN=$(python3 -c "
   import json, pathlib
   cfg = pathlib.Path.home() / '.config/joplin-desktop/settings.json'
   try:
       print(json.loads(cfg.read_text()).get('api.token', ''))
   except Exception:
       pass
   " 2>/dev/null)
   ```

   Then reload your shell: `source ~/.bashrc`

> **Tip:** If you ever expose the token accidentally, rotate it from
> the same screen with **Renew authorisation token**. The `~/.bashrc`
> snippet above will automatically pick up the new value.

## Installation

Clone this repo and put the script (or a symlink to it) somewhere on
your `$PATH`:

```bash
git clone https://github.com/NormG/Joplin-dictate.git ~/projects/joplin-dictate
mkdir -p ~/bin
ln -s ~/projects/joplin-dictate/joplin-dictate.sh ~/bin/joplin-dictate.sh
echo 'export PATH="$HOME/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

## Usage

Start dictating, then press **Ctrl-C** to stop recording. The script
will transcribe the clip and create a Joplin item.

```bash
joplin-dictate.sh                       # plain note in default notebook
joplin-dictate.sh -t "Meeting notes"    # custom title
joplin-dictate.sh -p <FOLDER_ID>        # specific notebook
joplin-dictate.sh -d                    # create a to-do (checkbox)
joplin-dictate.sh -D "tomorrow 9am"     # to-do with due date (implies -d)
joplin-dictate.sh -h                    # help
```

### Recording tips

- **Wait a moment before speaking.** `arecord` needs ~0.5 s to
  initialise. Starting to speak immediately may clip the first word.
- **Speak for at least 2–3 seconds.** Very short clips often produce
  only silence or hallucination tokens, which are filtered out and
  reported as `No speech detected.`
- **Speak clearly and close to the microphone.** whisper.cpp works best
  with a clean signal; background noise can reduce accuracy.
- **Use a larger model for better accuracy.** The default `base.en` is
  fast but approximate. `small.en` or `medium.en` give noticeably better
  results at the cost of a few extra seconds of transcription time.

### Flags

- `-t TITLE` — use a custom title instead of the first sentence.
- `-p FOLDER_ID` — create the item in a specific notebook. Find IDs with:

  ```bash
  curl -s "http://127.0.0.1:41184/folders?token=$JOPLIN_TOKEN" | jq
  ```

- `-d` — create a Joplin **to-do** (checkbox-style) instead of a regular note.
- `-D "<date phrase>"` — set a due date. Accepts anything GNU `date -d`
  understands (`"tomorrow 9am"`, `"next monday"`, `"2026-05-01 18:00"`,
  RFC3339 timestamps, etc.). Implies `-d`.
- `-h` — show the inline help.

### Environment overrides

- `JOPLIN_TOKEN` — **required**, the Web Clipper authorisation token.
- `JOPLIN_HOST` — default `http://127.0.0.1:41184`.
- `WHISPER_DIR` — default `~/whisper.cpp`.
- `WHISPER_MODEL` — default `$WHISPER_DIR/models/ggml-base.en.bin`.

For a more accurate model, point `WHISPER_MODEL` at a larger one
(e.g. `ggml-small.en.bin`, `ggml-medium.en.bin`).

## How it works

1. `arecord` records mono 16 kHz PCM into a temp WAV (the format
   whisper.cpp expects natively).
2. Pressing `Ctrl-C` (shell) or clicking **Stop** (GUI) stops the
   recording cleanly; the script catches `SIGINT` so the rest of the
   pipeline still runs.
3. `whisper-cli` produces a `.txt` transcript.
4. Known whisper hallucination tokens (`[Blank Audio]`, `[noise]`,
   `[Music]`, etc.) are stripped from the transcript. If nothing
   remains, the script prints `No speech detected.` and exits without
   creating a note.
5. `jq` builds a JSON payload with the transcript as `body` and either
   the first line or `-t TITLE` as `title`.
6. `curl` POSTs to `${JOPLIN_HOST}/notes?token=${JOPLIN_TOKEN}`. With
   `-d`/`-D`, `is_todo` and `todo_due` are added to the payload — Joplin
   uses the same `/notes` endpoint for notes and to-dos.
7. The temp directory is deleted on exit (success or failure).

## GUI (GTK)

A GTK 3 front-end is available as `joplin-dictate-gui.py`. It provides a
full-featured window with a notebook picker, title field, to-do checkbox,
due-date entry, and a Start / Stop recording button.

### Extra requirement

```bash
sudo dnf install python3-gobject          # Fedora
sudo apt install python3-gi gir1.2-gtk-3.0  # Debian / Ubuntu
```

### Running the GUI

```bash
python3 joplin-dictate-gui.py
# or, if you've made it executable:
./joplin-dictate-gui.py
```

All environment variables (`JOPLIN_TOKEN`, `JOPLIN_HOST`, `WHISPER_DIR`,
`WHISPER_MODEL`) work exactly the same as with the shell script.

### Startup checks

The GUI runs a pre-flight check on every launch. The record button stays
disabled until every check passes. The status bar reports each failure
with a specific, actionable message:

- `⚠ Joplin not found` — install Joplin (see [Requirements](#requirements))
- `⚠ JOPLIN_TOKEN not set` — add the export to `~/.bashrc` (see [Configuring Joplin](#configuring-joplin))
- `⚠ whisper-cli not found` — build whisper.cpp (see [Requirements](#requirements))
- `⚠ Whisper model not found` — download a model with `download-ggml-model.sh`
- `⚠ Joplin Web Clipper not reachable` — start Joplin and enable Web Clipper
- `Ready.` — all checks passed, the button is enabled

### GNOME launcher

To add Joplin Dictate to your application grid and Activities search:

```bash
# 1. Install the icon (bundled in the repo)
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp joplin-dictate.svg ~/.local/share/icons/hicolor/scalable/apps/

# 2. Create the wrapper script (reads JOPLIN_TOKEN automatically)
mkdir -p ~/.local/bin
cat > ~/.local/bin/joplin-dictate <<'SH'
#!/usr/bin/env bash
JOPLIN_TOKEN=$(python3 -c "
import json, pathlib
cfg = pathlib.Path.home() / '.config/joplin-desktop/settings.json'
try:
    print(json.loads(cfg.read_text()).get('api.token', ''))
except Exception:
    pass
" 2>/dev/null)
export JOPLIN_TOKEN
exec python3 "$HOME/projects/joplin-dictate/joplin-dictate-gui.py" "$@"
SH
chmod +x ~/.local/bin/joplin-dictate

# 3. Install the .desktop entry
mkdir -p ~/.local/share/applications
cat > ~/.local/share/applications/joplin-dictate.desktop <<'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=Joplin Dictate
GenericName=Voice Note Recorder
Comment=Record voice notes and send them straight to Joplin
Exec=/home/USER/.local/bin/joplin-dictate
Icon=joplin-dictate
Terminal=false
Categories=AudioVideo;Utility;
Keywords=joplin;note;dictate;voice;microphone;record;transcribe;whisper;
StartupNotify=true
EOF
# Replace USER with your actual username
sed -i "s|/home/USER/|$HOME/|g" ~/.local/share/applications/joplin-dictate.desktop

update-desktop-database ~/.local/share/applications/
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/
```

> **Note:** Edit the `Exec=` path in the wrapper script if you cloned
> the repo somewhere other than `~/projects/joplin-dictate`.

## Tips

- Bind `joplin-dictate.sh` to a global keyboard shortcut in your desktop
  environment so dictation is one keypress away.
- For a more accurate model, point `WHISPER_MODEL` at a larger binary
  (`ggml-small.en.bin`, `ggml-medium.en.bin`).
- For real-time/streaming transcription, look at whisper.cpp's
  `whisper-stream` example.

## Troubleshooting

**Joplin**

- **`Joplin does not appear to be installed`** (shell) /
  **`⚠ Joplin not found`** (GUI) —
  install Joplin using the official script (see [Requirements](#requirements)).
  On Fedora, install `fuse-libs` first: `sudo dnf install -y fuse-libs`.
- **`Cannot reach Joplin Web Clipper`** (shell) /
  **`⚠ Joplin Web Clipper not reachable`** (GUI) —
  Joplin is not running, or Web Clipper is disabled.
  Start Joplin and go to Tools → Options → Web Clipper → Enable.
- **`JOPLIN_TOKEN is not set`** (shell) /
  **`⚠ JOPLIN_TOKEN not set`** (GUI) —
  add the dynamic export snippet to `~/.bashrc`
  (see [Configuring Joplin](#configuring-joplin)) and run `source ~/.bashrc`.

**whisper.cpp**

- **`Required command not found: …/whisper-cli`** (shell) /
  **`⚠ whisper-cli not found`** (GUI) —
  whisper.cpp has not been built yet. Run:
  ```bash
  cmake -S ~/whisper.cpp -B ~/whisper.cpp/build -DCMAKE_BUILD_TYPE=Release
  cmake --build ~/whisper.cpp/build -j$(nproc)
  ```
  If `~/whisper.cpp` does not exist, clone it first (see [Requirements](#requirements)).
- **`Whisper model not found`** (shell) /
  **`⚠ Whisper model not found`** (GUI) —
  download a model:
  ```bash
  bash ~/whisper.cpp/models/download-ggml-model.sh base.en
  ```
  Or point `WHISPER_MODEL` at an existing `.bin` file.

**Audio**

- **`No speech detected`** — the recording captured only silence,
  background noise, or a whisper hallucination token (`[Blank Audio]`,
  `[noise]`, etc. — these are filtered automatically and never saved
  as a note). Common causes and fixes:
  - *Too short* — speak for at least 2–3 seconds before stopping.
  - *Wrong timing* — wait ~0.5 s after starting before speaking.
  - *Mic not working* — verify with:
    ```bash
    arecord -d 3 /tmp/test.wav && aplay /tmp/test.wav
    ```
  - *Wrong device* — list available devices with `arecord -l` and
    pass `-D plughw:CARD,DEV` to `arecord` (edit the script or wrap it).
- **Wrong language / poor accuracy** — switch to a larger model
  (`small.en`, `medium.en`) or, for non-English audio, a multilingual
  model (`small`, `medium`) and pass `-l <lang>` to whisper-cli via
  `WHISPER_MODEL` pointing at the multilingual `.bin`.

## Contributing

Issues and pull requests are welcome at
<https://github.com/NormG/Joplin-dictate>.

If you'd like to contribute a change:

1. Fork the repository and create a feature branch:

   ```bash
   git checkout -b my-feature
   ```

2. Make your change. Please follow the existing style:
   - POSIX-friendly `bash` with `set -euo pipefail`.
   - Quote variables, prefer `[[ ... ]]` over `[ ... ]`.
   - Keep the script self-contained — no new runtime dependencies
     beyond `arecord`, `curl`, `jq`, and `whisper.cpp` unless absolutely
     necessary.

3. Validate the script before committing:

   ```bash
   bash -n joplin-dictate.sh          # syntax check
   shellcheck joplin-dictate.sh       # optional but recommended
   joplin-dictate.sh -h               # smoke-test the help output
   ```

4. Commit with a clear message describing *why* the change is needed,
   then open a pull request against `main`.

For non-trivial changes, please open an issue first to discuss the
approach.

## License

MIT.
