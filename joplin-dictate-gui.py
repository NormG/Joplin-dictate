#!/usr/bin/env python3
"""
joplin-dictate-gui.py
GTK 3 front-end for joplin-dictate.sh

Requirements (in addition to joplin-dictate.sh dependencies):
  sudo dnf install python3-gobject          # Fedora
  sudo apt install python3-gi gir1.2-gtk-3.0  # Debian / Ubuntu
"""

import gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GLib, Pango

import json
import os
import signal
import subprocess
import threading
import urllib.request
import urllib.error
from pathlib import Path


JOPLIN_HOST  = os.environ.get('JOPLIN_HOST',  'http://127.0.0.1:41184')
JOPLIN_TOKEN = os.environ.get('JOPLIN_TOKEN', '')
SCRIPT       = Path(__file__).resolve().parent / 'joplin-dictate.sh'

# Lines in the script's output worth surfacing in the status bar
_RELEVANT_PREFIXES = ('Created', 'Title:', 'Due:', 'Error', 'No speech')


def _fetch_notebooks():
    """Return [(id, title), …] sorted by title, or raise on error."""
    url = f'{JOPLIN_HOST}/folders?token={JOPLIN_TOKEN}'
    with urllib.request.urlopen(url, timeout=4) as r:
        raw = json.loads(r.read())
    items = raw.get('items', raw) if isinstance(raw, dict) else raw
    return sorted(
        [(nb['id'], nb['title']) for nb in items],
        key=lambda p: p[1].lower(),
    )


def _summarise_output(text: str) -> str:
    """Return the most user-relevant lines from the script's combined output."""
    lines = text.splitlines()
    relevant = [l for l in lines
                if any(l.startswith(p) for p in _RELEVANT_PREFIXES)]
    return '  |  '.join(relevant) if relevant else (text.strip() or 'Done.')


class DictateWindow(Gtk.Window):

    def __init__(self):
        super().__init__(title='Joplin Dictate')
        self.set_border_width(14)
        self.set_default_size(420, -1)
        self.set_resizable(False)
        self.connect('destroy', self._on_destroy)

        self._proc:      subprocess.Popen | None = None
        self._recording: bool = False

        self._build_ui()
        GLib.idle_add(self._check_env)

    # ── UI construction ───────────────────────────────────────────────────

    def _build_ui(self) -> None:
        vbox = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        self.add(vbox)

        grid = Gtk.Grid(column_spacing=8, row_spacing=8)
        vbox.pack_start(grid, False, False, 0)

        def lbl(text: str) -> Gtk.Label:
            return Gtk.Label(label=text, xalign=1.0)

        # Notebook
        grid.attach(lbl('Notebook:'), 0, 0, 1, 1)
        self._nb_combo = Gtk.ComboBoxText()
        self._nb_combo.append('', '— default notebook —')
        self._nb_combo.set_active(0)
        self._nb_combo.set_hexpand(True)
        grid.attach(self._nb_combo, 1, 0, 1, 1)

        # Title
        grid.attach(lbl('Title:'), 0, 1, 1, 1)
        self._title_entry = Gtk.Entry()
        self._title_entry.set_placeholder_text('Auto (first line of transcript)')
        self._title_entry.set_hexpand(True)
        grid.attach(self._title_entry, 1, 1, 1, 1)

        # To-do checkbox
        self._todo_check = Gtk.CheckButton(label='Create as to-do')
        self._todo_check.connect('toggled', self._on_todo_toggled)
        grid.attach(self._todo_check, 1, 2, 1, 1)

        # Due date (only active when to-do is ticked)
        grid.attach(lbl('Due date:'), 0, 3, 1, 1)
        self._due_entry = Gtk.Entry()
        self._due_entry.set_placeholder_text('"tomorrow 9am",  "next monday",  …')
        self._due_entry.set_sensitive(False)
        self._due_entry.set_hexpand(True)
        grid.attach(self._due_entry, 1, 3, 1, 1)

        # Record / Stop button
        self._rec_btn = Gtk.Button(label='▶  Start Recording')
        self._rec_btn.get_style_context().add_class('suggested-action')
        self._rec_btn.set_size_request(-1, 52)
        self._rec_btn.connect('clicked', self._on_rec_clicked)
        vbox.pack_start(self._rec_btn, False, False, 4)

        # Status bar
        self._status_lbl = Gtk.Label(label='', xalign=0)
        self._status_lbl.set_ellipsize(Pango.EllipsizeMode.END)
        self._status_lbl.get_style_context().add_class('dim-label')
        vbox.pack_start(self._status_lbl, False, False, 0)

        self.show_all()

    # ── signal / event handlers ───────────────────────────────────────────

    def _on_todo_toggled(self, btn: Gtk.CheckButton) -> None:
        active = btn.get_active()
        self._due_entry.set_sensitive(active)
        if not active:
            self._due_entry.set_text('')

    def _on_rec_clicked(self, _btn: Gtk.Button) -> None:
        if self._recording:
            self._stop_recording()
        else:
            self._start_recording()

    def _on_destroy(self, _win: Gtk.Window) -> None:
        self._kill_proc(use_sigint=False)
        Gtk.main_quit()

    # ── environment check (runs once after window is shown) ───────────────

    @staticmethod
    def _joplin_installed() -> bool:
        """Return True if a Joplin installation can be found on this machine."""
        import shutil
        home = Path.home()
        # AppImage — standard location used by the official install script
        candidates = [home / '.joplin' / 'Joplin.AppImage']
        # AppImage in home dir or ~/Applications
        for pattern in ('Joplin*.AppImage', 'Applications/Joplin*.AppImage'):
            candidates.extend(home.glob(pattern))
        if any(p.exists() for p in candidates):
            return True
        # Flatpak
        try:
            out = subprocess.run(
                ['flatpak', 'list', '--columns=application'],
                capture_output=True, text=True, timeout=3,
            ).stdout
            if 'joplin' in out.lower():
                return True
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass
        # System / PATH executable (e.g. npm install -g joplin)
        return shutil.which('joplin') is not None

    def _check_env(self) -> bool:
        if not self._joplin_installed():
            self._set_status(
                '⚠  Joplin not found — install it from joplinapp.org first.'
            )
            self._rec_btn.set_sensitive(False)
            return False
        if not JOPLIN_TOKEN:
            self._set_status('⚠  JOPLIN_TOKEN not set — export it before running.')
            self._rec_btn.set_sensitive(False)
            return False
        if not SCRIPT.exists():
            self._set_status(f'⚠  Script not found: {SCRIPT}')
            self._rec_btn.set_sensitive(False)
            return False
        threading.Thread(target=self._load_notebooks_bg, daemon=True).start()
        return False  # one-shot idle callback

    def _load_notebooks_bg(self) -> None:
        try:
            notebooks = _fetch_notebooks()
            GLib.idle_add(self._populate_notebooks, notebooks)
        except Exception as exc:
            GLib.idle_add(
                self._set_status,
                f'Notebook list unavailable ({exc}); default will be used.',
            )

    def _populate_notebooks(self, notebooks: list) -> bool:
        for nb_id, title in notebooks:
            self._nb_combo.append(nb_id, title)
        return False

    # ── recording control ─────────────────────────────────────────────────

    def _start_recording(self) -> None:
        cmd = [str(SCRIPT)]

        title = self._title_entry.get_text().strip()
        if title:
            cmd += ['-t', title]

        nb_id = self._nb_combo.get_active_id() or ''
        if nb_id:
            cmd += ['-p', nb_id]

        if self._todo_check.get_active():
            due = self._due_entry.get_text().strip()
            cmd += (['-D', due] if due else ['-d'])

        try:
            self._proc = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,   # merge so errors appear in status
                start_new_session=True,     # own process group → killpg works
            )
        except OSError as exc:
            self._set_status(f'Launch error: {exc}')
            return

        self._recording = True
        self._rec_btn.set_label('⏹  Stop Recording')
        ctx = self._rec_btn.get_style_context()
        ctx.remove_class('suggested-action')
        ctx.add_class('destructive-action')
        self._set_status('Recording…  press Stop when done.')

        threading.Thread(target=self._watch_proc, daemon=True).start()

    def _stop_recording(self) -> None:
        """Send SIGINT to the whole process group so arecord exits cleanly."""
        self._kill_proc(use_sigint=True)
        self._recording = False
        self._rec_btn.set_label('▶  Start Recording')
        ctx = self._rec_btn.get_style_context()
        ctx.remove_class('destructive-action')
        ctx.add_class('suggested-action')
        self._rec_btn.set_sensitive(False)   # re-enabled after script finishes
        self._set_status('Transcribing and creating note…')

    def _watch_proc(self) -> None:
        """Background thread: block until joplin-dictate.sh exits, then update UI."""
        stdout, _ = self._proc.communicate()
        rc        = self._proc.returncode
        text      = stdout.decode(errors='replace').strip()

        if rc == 0:
            msg = _summarise_output(text)
        else:
            summary = _summarise_output(text) if text else f'exit code {rc}'
            msg = f'Error: {summary}'

        GLib.idle_add(self._after_proc, msg)

    def _after_proc(self, msg: str) -> bool:
        self._set_status(msg)
        self._rec_btn.set_sensitive(True)
        self._rec_btn.set_label('▶  Start Recording')
        ctx = self._rec_btn.get_style_context()
        ctx.remove_class('destructive-action')
        ctx.add_class('suggested-action')
        self._recording = False
        return False

    # ── helpers ────────────────────────────────────────────────────────────

    def _kill_proc(self, *, use_sigint: bool) -> None:
        if self._proc and self._proc.poll() is None:
            try:
                sig = signal.SIGINT if use_sigint else signal.SIGTERM
                os.killpg(self._proc.pid, sig)
            except ProcessLookupError:
                pass

    def _set_status(self, text: str) -> bool:
        self._status_lbl.set_text(str(text))
        return False


def main() -> None:
    win = DictateWindow()
    Gtk.main()


if __name__ == '__main__':
    main()
