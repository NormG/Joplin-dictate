use anyhow::{Context, Result, anyhow, bail};
use chrono::{Local, NaiveDateTime, TimeZone};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct Config {
    pub whisper_dir: PathBuf,
    pub whisper_model: PathBuf,
    pub whisper_bin: PathBuf,
    pub joplin_host: String,
    pub joplin_token: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
        let whisper_dir = env::var_os("WHISPER_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("whisper.cpp"));
        let whisper_model = env::var_os("WHISPER_MODEL")
            .map(PathBuf::from)
            .unwrap_or_else(|| whisper_dir.join("models/ggml-base.en.bin"));
        let whisper_bin = whisper_dir.join("build/bin/whisper-cli");
        let joplin_host =
            env::var("JOPLIN_HOST").unwrap_or_else(|_| "http://127.0.0.1:41184".to_string());
        let joplin_token = env::var("JOPLIN_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(read_joplin_token_from_settings)
            .unwrap_or_default();

        Ok(Self {
            whisper_dir,
            whisper_model,
            whisper_bin,
            joplin_host,
            joplin_token,
        })
    }

    pub fn require_ready(&self) -> Result<()> {
        if !joplin_installed() {
            bail!("Joplin not found — install it from https://joplinapp.org first");
        }
        if self.joplin_token.trim().is_empty() {
            bail!("JOPLIN_TOKEN not set — enable Joplin Web Clipper and export the token");
        }
        if !self.whisper_bin.exists() {
            bail!(
                "whisper-cli not found — build whisper.cpp first (missing: {})",
                self.whisper_bin.display()
            );
        }
        if !self.whisper_model.exists() {
            bail!("Whisper model not found: {}", self.whisper_model.display());
        }
        ping_joplin(self)?;
        Ok(())
    }
}

fn read_joplin_token_from_settings() -> Option<String> {
    let settings = dirs::home_dir()?.join(".config/joplin-desktop/settings.json");
    let text = fs::read_to_string(settings).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("api.token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

pub fn joplin_installed() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };

    let direct_candidates = [
        home.join(".joplin/Joplin.AppImage"),
        home.join("Joplin.AppImage"),
    ];
    if direct_candidates.iter().any(|p| p.exists()) {
        return true;
    }

    if let Ok(entries) = fs::read_dir(home.join("Applications"))
        && entries.flatten().any(|e| {
            e.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("joplin")
                && e.path().extension().is_some_and(|ext| ext == "AppImage")
        })
    {
        return true;
    }

    if which::which("joplin").is_ok() {
        return true;
    }

    Command::new("flatpak")
        .args(["list", "--columns=application"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|s| s.to_ascii_lowercase().contains("joplin"))
}

pub fn ping_joplin(config: &Config) -> Result<()> {
    let url = format!("{}/ping", config.joplin_host.trim_end_matches('/'));
    let text = Client::new()
        .get(url)
        .send()
        .context("Cannot reach Joplin Web Clipper")?
        .error_for_status()
        .context("Joplin Web Clipper returned an error")?
        .text()
        .context("Cannot read Joplin Web Clipper response")?;
    if !text.contains("JoplinClipperServer") {
        bail!("Unexpected Joplin Web Clipper ping response: {text}");
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Folder {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct FolderPage {
    items: Vec<Folder>,
}

pub fn list_folders(config: &Config) -> Result<Vec<Folder>> {
    let url = api_url(config, "folders");
    let page: FolderPage = Client::new()
        .get(url)
        .send()
        .context("Failed to list Joplin notebooks")?
        .error_for_status()
        .context("Joplin notebook list returned an error")?
        .json()
        .context("Failed to parse Joplin notebook list")?;
    let mut folders = page.items;
    folders.sort_by_key(|f| f.title.to_ascii_lowercase());
    Ok(folders)
}

fn api_url(config: &Config, path: &str) -> String {
    format!(
        "{}/{}?token={}",
        config.joplin_host.trim_end_matches('/'),
        path.trim_start_matches('/'),
        urlencoding::encode(&config.joplin_token)
    )
}

#[derive(Debug, Clone)]
pub struct CreateOptions {
    pub parent_id: Option<String>,
    pub title: Option<String>,
    pub is_todo: bool,
    pub due: Option<String>,
    pub audio_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CreatedNote {
    pub id: String,
    pub title: String,
    pub is_todo: bool,
    pub due_human: Option<String>,
    pub body: String,
}

#[derive(Debug, Deserialize)]
struct CreateResponse {
    id: Option<String>,
    title: Option<String>,
}

pub fn run_workflow(config: &Config, options: &CreateOptions) -> Result<Option<CreatedNote>> {
    config.require_ready()?;
    // Temp dir is used for transcription output only.
    // When a pre-recorded audio_file is provided we use it directly rather
    // than copying it, which avoids cross-device copy failures.
    let temp = TempDir::new().context("Failed to create temporary directory")?;
    let txt_base = temp.path().join("recording");

    let wav: PathBuf = if let Some(audio_file) = &options.audio_file {
        audio_file.clone()
    } else {
        let wav = temp.path().join("recording.wav");
        record_audio(&wav)?;
        wav
    };

    if fs::metadata(&wav).map(|m| m.len()).unwrap_or(0) == 0 {
        bail!("No audio captured");
    }

    transcribe(config, &wav, &txt_base)?;
    let transcript_path = txt_base.with_extension("txt");
    let raw = fs::read_to_string(&transcript_path).with_context(|| {
        format!(
            "Transcription output missing: {}",
            transcript_path.display()
        )
    })?;
    let text = filter_transcript(&raw);
    if text.trim().is_empty() {
        return Ok(None);
    }

    let title = options
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            let first = text
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>();
            if first.trim().is_empty() {
                format!("Dictation {}", Local::now().format("%Y-%m-%d %H:%M"))
            } else {
                first
            }
        });

    let due = match options.due.as_deref() {
        Some(raw_due) if !raw_due.trim().is_empty() => Some(parse_due(raw_due)?),
        _ => None,
    };

    let mut body = text.clone();
    if let Some((_, due_human)) = &due {
        body = format!("Due: {due_human}\n\n{text}");
    }

    let created = create_joplin_note(config, options, &title, &body, due.as_ref())?;
    Ok(Some(CreatedNote {
        id: created
            .id
            .ok_or_else(|| anyhow!("Joplin response did not include note ID"))?,
        title: created.title.unwrap_or(title),
        is_todo: options.is_todo || due.is_some(),
        due_human: due.map(|(_, human)| human),
        body,
    }))
}

fn record_audio(wav: &Path) -> Result<()> {
    println!("Recording... press Ctrl-C to stop.");
    let mut child = Command::new("arecord")
        .args(["-q", "-f", "S16_LE", "-c", "1", "-r", "16000"])
        .arg(wav)
        .spawn()
        .context("Failed to start arecord")?;

    // Let Ctrl-C stop arecord while keeping this process alive long enough to
    // continue transcription, matching the original Bash script behavior.
    let _ = ctrlc::set_handler(|| {});
    let _ = child.wait().context("Failed while waiting for arecord")?;
    Ok(())
}

fn transcribe(config: &Config, wav: &Path, txt_base: &Path) -> Result<()> {
    println!("Transcribing...");
    let status = Command::new(&config.whisper_bin)
        .arg("-m")
        .arg(&config.whisper_model)
        .arg("-f")
        .arg(wav)
        .arg("-otxt")
        .arg("-of")
        .arg(txt_base)
        .arg("-nt")
        .arg("--no-fallback")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to start whisper-cli")?;
    if !status.success() {
        bail!("whisper-cli failed with status {status}");
    }
    Ok(())
}

pub fn filter_transcript(raw: &str) -> String {
    let mut text = raw.to_string();
    let replacements = [
        "[Blank Audio]",
        "[BLANK_AUDIO]",
        "[ Silence ]",
        "[ silence ]",
        "[Silence]",
        "[noise]",
        "[Noise]",
        "[Music]",
        "[music]",
        "(silence)",
        "(Silence)",
    ];
    for needle in replacements {
        text = text.replace(needle, "");
    }
    text.trim().to_string()
}

pub fn parse_due(raw: &str) -> Result<(i64, String)> {
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S"))
        .with_context(|| format!("Could not parse due date: {raw}"))?;
    let dt = Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| anyhow!("Due date is ambiguous or invalid in local time: {raw}"))?;
    let ms = dt.timestamp_millis();
    let human = dt.format("%a %-d %b %Y %H:%M").to_string();
    Ok((ms, human))
}

fn create_joplin_note(
    config: &Config,
    options: &CreateOptions,
    title: &str,
    body: &str,
    due: Option<&(i64, String)>,
) -> Result<CreateResponse> {
    #[derive(Debug, Serialize)]
    struct Payload<'a> {
        title: &'a str,
        body: &'a str,
        is_todo: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo_due: Option<i64>,
    }

    let payload = Payload {
        title,
        body,
        is_todo: if options.is_todo || due.is_some() {
            1
        } else {
            0
        },
        parent_id: options.parent_id.as_deref().filter(|s| !s.is_empty()),
        todo_due: due.map(|(ms, _)| *ms),
    };

    Client::new()
        .post(api_url(config, "notes"))
        .json(&payload)
        .send()
        .context("Failed to create Joplin note")?
        .error_for_status()
        .context("Joplin note creation returned an error")?
        .json()
        .context("Failed to parse Joplin note creation response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_known_tokens_and_preserves_speech() {
        assert_eq!(filter_transcript("[Blank Audio]"), "");
        assert_eq!(filter_transcript("[ Silence ] Hello [noise]"), "Hello");
        assert_eq!(filter_transcript("[Music] Hello world"), "Hello world");
    }

    #[test]
    fn parses_iso_due_date() {
        let (_ms, human) = parse_due("2026-08-01 14:00").unwrap();
        assert!(human.contains("2026 14:00"));
    }
}
