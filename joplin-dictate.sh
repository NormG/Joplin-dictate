#!/usr/bin/env bash
# joplin-dictate.sh
# Record audio, transcribe with whisper.cpp, and create a Joplin note
# via the Web Clipper API.
#
# Requirements:
#   - whisper.cpp built at $WHISPER_DIR (default: ~/whisper.cpp)
#   - A whisper model at $MODEL (default: ggml-base.en.bin)
#   - arecord (alsa-utils), curl, jq
#   - Joplin Web Clipper enabled (Tools -> Options -> Web Clipper)
#   - $JOPLIN_TOKEN exported with the Web Clipper authorization token
#
# Usage:
#   joplin-dictate.sh                 # new note in default notebook
#   joplin-dictate.sh -p FOLDER_ID    # new note in a specific notebook
#   joplin-dictate.sh -t "My title"   # use a custom title
#   joplin-dictate.sh -d              # create a to-do instead of a note
#   joplin-dictate.sh -d -D "tomorrow 9am"   # to-do with due date
#
# Press Ctrl-C to stop recording; transcription will then be sent.

set -euo pipefail

WHISPER_DIR="${WHISPER_DIR:-$HOME/whisper.cpp}"
MODEL="${WHISPER_MODEL:-$WHISPER_DIR/models/ggml-base.en.bin}"
WHISPER_BIN="$WHISPER_DIR/build/bin/whisper-cli"
JOPLIN_HOST="${JOPLIN_HOST:-http://127.0.0.1:41184}"

PARENT_ID=""
CUSTOM_TITLE=""
IS_TODO=0
DUE_DATE_RAW=""

while getopts ":p:t:dD:h" opt; do
    case "$opt" in
        p) PARENT_ID="$OPTARG" ;;
        t) CUSTOM_TITLE="$OPTARG" ;;
        d) IS_TODO=1 ;;
        D) DUE_DATE_RAW="$OPTARG"; IS_TODO=1 ;;
        h)
            sed -n '2,20p' "$0"
            exit 0
            ;;
        *)
            echo "Unknown option: -$OPTARG" >&2
            exit 2
            ;;
    esac
done

# --- sanity checks --------------------------------------------------------
if [[ -z "${JOPLIN_TOKEN:-}" ]]; then
    echo "JOPLIN_TOKEN is not set. Export it from the Joplin Web Clipper settings." >&2
    exit 1
fi

for cmd in arecord curl jq "$WHISPER_BIN"; do
    if ! command -v "$cmd" >/dev/null 2>&1 && [[ ! -x "$cmd" ]]; then
        echo "Required command not found: $cmd" >&2
        exit 1
    fi
done

if [[ ! -f "$MODEL" ]]; then
    echo "Whisper model not found: $MODEL" >&2
    exit 1
fi

# Confirm Joplin is reachable
if ! curl -fsS "$JOPLIN_HOST/ping" >/dev/null; then
    echo "Cannot reach Joplin Web Clipper at $JOPLIN_HOST." >&2
    echo "Make sure Joplin is running and the Web Clipper service is enabled." >&2
    exit 1
fi

# --- temp files -----------------------------------------------------------
TMPDIR_RUN="$(mktemp -d)"
WAV="$TMPDIR_RUN/recording.wav"
TXT_BASE="$TMPDIR_RUN/recording"
trap 'rm -rf "$TMPDIR_RUN"' EXIT

# --- record ---------------------------------------------------------------
echo "Recording... press Ctrl-C to stop."
# Catch SIGINT so arecord stops cleanly without aborting the script.
trap 'true' INT
arecord -q -f S16_LE -c 1 -r 16000 "$WAV" || true
trap - INT

if [[ ! -s "$WAV" ]]; then
    echo "No audio captured." >&2
    exit 1
fi

# --- transcribe -----------------------------------------------------------
echo "Transcribing..."
"$WHISPER_BIN" \
    -m "$MODEL" \
    -f "$WAV" \
    -otxt \
    -of "$TXT_BASE" \
    -nt \
    >/dev/null 2>&1

if [[ ! -f "${TXT_BASE}.txt" ]]; then
    echo "Transcription failed: no output file produced." >&2
    exit 1
fi

TEXT="$(sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' "${TXT_BASE}.txt")"

if [[ -z "${TEXT// }" ]]; then
    echo "No speech detected."
    exit 0
fi

# --- build payload --------------------------------------------------------
if [[ -n "$CUSTOM_TITLE" ]]; then
    TITLE="$CUSTOM_TITLE"
else
    TITLE="$(printf '%s' "$TEXT" | head -n1 | cut -c1-80)"
    [[ -z "$TITLE" ]] && TITLE="Dictation $(date '+%Y-%m-%d %H:%M')"
fi

# Convert optional due-date phrase to milliseconds since epoch
DUE_MS=0
if [[ -n "$DUE_DATE_RAW" ]]; then
    if ! DUE_EPOCH=$(date -d "$DUE_DATE_RAW" +%s 2>/dev/null); then
        echo "Could not parse due date: $DUE_DATE_RAW" >&2
        exit 1
    fi
    DUE_MS=$(( DUE_EPOCH * 1000 ))
fi

JSON=$(jq -n \
    --arg t "$TITLE" \
    --arg b "$TEXT" \
    --arg p "$PARENT_ID" \
    --argjson todo "$IS_TODO" \
    --argjson due "$DUE_MS" \
    '
      {title:$t, body:$b, is_todo:$todo}
      + (if $p == "" then {} else {parent_id:$p} end)
      + (if $due == 0 then {} else {todo_due:$due} end)
    ')

# --- send to Joplin -------------------------------------------------------
RESPONSE=$(curl -fsS -X POST \
    "$JOPLIN_HOST/notes?token=${JOPLIN_TOKEN}" \
    -H 'Content-Type: application/json' \
    -d "$JSON")

NOTE_ID=$(printf '%s' "$RESPONSE" | jq -r '.id // empty')

if [[ -z "$NOTE_ID" ]]; then
    echo "Failed to create note. Response:" >&2
    echo "$RESPONSE" >&2
    exit 1
fi

if [[ "$IS_TODO" -eq 1 ]]; then
    echo "Created Joplin to-do: $NOTE_ID"
else
    echo "Created Joplin note: $NOTE_ID"
fi
echo "Title: $TITLE"
[[ "$DUE_MS" -gt 0 ]] && echo "Due:   $(date -d "@$((DUE_MS/1000))")"
