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

On Fedora the system packages can be installed with:

```bash
sudo dnf install -y git cmake gcc-c++ make sox alsa-utils jq curl
```

Build whisper.cpp and download a model:

```bash
git clone https://github.com/ggml-org/whisper.cpp.git ~/whisper.cpp
cd ~/whisper.cpp
cmake -B build
cmake --build build -j --config Release
bash ./models/download-ggml-model.sh base.en   # or small.en, medium.en, etc.
```

## Configuring Joplin

1. Open Joplin → **Tools → Options → Web Clipper**.
2. Click **Enable Web Clipper Service**. The default port is `41184`.
3. Copy the **authorisation token** shown on that screen.
4. Export it in your shell (e.g. add to `~/.bashrc`):

   ```bash
   export JOPLIN_TOKEN="paste-token-here"
   ```

> **Tip:** If you ever paste the token somewhere public, rotate it from
> the same screen with **Renew authorisation token**.

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
2. Pressing `Ctrl-C` stops the recording cleanly; the script catches
   `SIGINT` so the rest of the pipeline still runs.
3. `whisper-cli` produces a `.txt` transcript.
4. `jq` builds a JSON payload with the transcript as `body` and either
   the first line or `-t TITLE` as `title`.
5. `curl` POSTs to `${JOPLIN_HOST}/notes?token=${JOPLIN_TOKEN}`. With
   `-d`/`-D`, `is_todo` and `todo_due` are added to the payload — Joplin
   uses the same `/notes` endpoint for notes and to-dos.
6. The temp directory is deleted on exit (success or failure).

## Tips

- If `arecord` picks the wrong microphone, find the right device with
  `arecord -l` and pass `-D plughw:CARD,DEV` (edit the script or wrap it).
- Bind `joplin-dictate.sh` to a global keyboard shortcut in your desktop
  environment so dictation is one keypress away.
- For real-time/streaming transcription, look at whisper.cpp's
  `whisper-stream` example.

## Troubleshooting

- **`Cannot reach Joplin Web Clipper at http://127.0.0.1:41184`** —
  Joplin is not running, or the Web Clipper service is disabled. Check
  Tools → Options → Web Clipper.
- **`JOPLIN_TOKEN is not set`** — export the token (see above).
- **`No speech detected`** — the recording was empty or only silence.
  Verify your mic with `arecord -d 3 test.wav && aplay test.wav`.
- **Wrong language / poor accuracy** — switch to a larger model
  (`small.en`, `medium.en`) or, for non-English audio, a multilingual
  model like `small`/`medium` and pass `-l <lang>` to whisper-cli.

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
