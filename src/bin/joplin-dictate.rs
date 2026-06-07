use anyhow::Result;
use clap::Parser;
use joplin_dictate::{Config, CreateOptions, run_workflow};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "joplin-dictate",
    version,
    about = "Record voice, transcribe locally with whisper.cpp, and create a Joplin note or to-do"
)]
struct Args {
    /// Create the note in a specific Joplin notebook/folder ID
    #[arg(short = 'p', long = "parent")]
    parent_id: Option<String>,

    /// Use a custom title instead of deriving one from the transcript
    #[arg(short = 't', long = "title")]
    title: Option<String>,

    /// Create a Joplin to-do instead of a regular note
    #[arg(short = 'd', long = "todo")]
    todo: bool,

    /// Set a due date for a to-do, in YYYY-MM-DD HH:MM format
    #[arg(short = 'D', long = "due")]
    due: Option<String>,

    /// Use an existing WAV file instead of recording from the microphone
    #[arg(long = "audio-file")]
    audio_file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::load()?;
    let options = CreateOptions {
        parent_id: args.parent_id,
        title: args.title,
        is_todo: args.todo || args.due.is_some(),
        due: args.due,
        audio_file: args.audio_file,
    };

    match run_workflow(&config, &options)? {
        Some(note) => {
            if note.is_todo {
                println!("Created Joplin to-do: {}", note.id);
            } else {
                println!("Created Joplin note: {}", note.id);
            }
            println!("Title: {}", note.title);
            if let Some(due) = note.due_human {
                println!("Due:   {due}");
            }
        }
        None => println!("No speech detected."),
    }

    Ok(())
}
