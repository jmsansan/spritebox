//! The `spritebox` command: validate and render spritebox files.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use spritebox::{AtlasOptions, Document, Error, ErrorCode, Limits, Position, PreviewOptions, png};

const USAGE: &str = "\
spritebox: 2D pixel art as plain text

Usage:
  spritebox check <file> [--json]
      Validate a file and print its diagnostics.
  spritebox render <file> --out <dir> [--scale <n>] [--padding <n>] [--extrude] [--json]
      Validate a file, then write atlas.png, spritebox.json (the atlas's
      metadata) and preview.png to <dir>.

<file> may be `-` for standard input.

Options:
  --json        Print diagnostics as JSON on standard output.
  --out <dir>   Directory for output files; created if missing.
  --scale <n>   Pixels per sprite pixel in the preview, 1..=64 (default 4).
  --padding <n> Empty pixels around each sprite in the atlas, 0..=64 (default 1).
  --extrude     Fill the atlas padding with copies of edge pixels.
  -h, --help    Print this help.
  -V, --version Print the version.

Exit status: 0 success, 1 the file has errors, 2 bad usage, 3 an I/O error.";

/// Exit statuses, stable for scripts and assistants.
const EXIT_INVALID: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_IO: u8 = 3;

struct Options {
    command: String,
    file: String,
    out: Option<PathBuf>,
    scale: u32,
    atlas: AtlasOptions,
    json: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") || args.first().is_some_and(|a| a == "help")
    {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("spritebox {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("spritebox: {message}\n\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    run(&options)
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut iter = args.iter();
    let command = iter.next().ok_or("missing command")?.clone();
    if command != "check" && command != "render" {
        return Err(format!("unknown command `{command}`"));
    }
    let mut file = None;
    let mut out = None;
    let mut scale = PreviewOptions::default().scale;
    let mut atlas = AtlasOptions::default();
    let mut json = false;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--out" => out = Some(PathBuf::from(iter.next().ok_or("--out needs a directory")?)),
            "--scale" => {
                let value = iter.next().ok_or("--scale needs a number")?;
                scale = value
                    .parse()
                    .ok()
                    .filter(|s| (1..=64).contains(s))
                    .ok_or(format!("--scale must be 1..=64, not `{value}`"))?;
            }
            "--padding" => {
                let value = iter.next().ok_or("--padding needs a number")?;
                atlas.padding = value
                    .parse()
                    .ok()
                    .filter(|p| *p <= 64)
                    .ok_or(format!("--padding must be 0..=64, not `{value}`"))?;
            }
            "--extrude" => atlas.extrude = true,
            _ if arg.starts_with("--") => return Err(format!("unknown option `{arg}`")),
            _ if file.is_none() => file = Some(arg.clone()),
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
    }
    let file = file.ok_or("missing <file>")?;
    if command == "render" && out.is_none() {
        return Err("render needs --out <dir>".into());
    }
    if command == "check" && out.is_some() {
        return Err("check writes no files; --out is for render".into());
    }
    if command == "check" && atlas != AtlasOptions::default() {
        return Err("--padding and --extrude are for render".into());
    }
    Ok(Options {
        command,
        file,
        out,
        scale,
        atlas,
        json,
    })
}

fn run(options: &Options) -> ExitCode {
    let text = match read_input(&options.file) {
        Ok(Ok(text)) => text,
        Ok(Err(error)) => return report(options, &[error], &[], EXIT_INVALID),
        Err(error) => return report(options, &[error], &[], EXIT_IO),
    };
    let limits = Limits::default();
    let errors = spritebox::check(&text, &limits);
    if !errors.is_empty() {
        return report(options, &errors, &[], EXIT_INVALID);
    }
    if options.command == "check" {
        return report(options, &[], &[], 0);
    }
    let document = match Document::parse_with(&text, &limits) {
        Ok(document) => document,
        Err(error) => return report(options, &[error], &[], EXIT_INVALID),
    };
    let out = options.out.as_deref().unwrap_or(Path::new("."));
    match render(&document, out, options, &limits) {
        Ok(written) => report(options, &[], &written, 0),
        Err(error) => {
            let status = if error.code == ErrorCode::Io {
                EXIT_IO
            } else {
                EXIT_INVALID
            };
            report(options, &[error], &[], status)
        }
    }
}

/// Reads the input; the inner error is text that is not UTF-8.
fn read_input(file: &str) -> Result<Result<String, Error>, Error> {
    let bytes = if file == "-" {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes)?;
        bytes
    } else {
        std::fs::read(file)
            .map_err(|e| Error::new(ErrorCode::Io, format!("Cannot read {file}: {e}")))?
    };
    Ok(String::from_utf8(bytes).map_err(|e| {
        let valid = &e.as_bytes()[..e.utf8_error().valid_up_to()];
        let valid = std::str::from_utf8(valid).unwrap_or_default();
        let line = valid.matches('\n').count() + 1;
        let column = valid
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count()
            + 1;
        Error::at(
            Position::new(line, column),
            ErrorCode::Syntax,
            "The text is not valid UTF-8",
        )
    }))
}

/// The files that `render` writes, in this order.
const ATLAS: &str = "atlas.png";
const METADATA: &str = "spritebox.json";
const PREVIEW: &str = "preview.png";

fn render(
    document: &Document,
    out: &Path,
    options: &Options,
    limits: &Limits,
) -> Result<Vec<PathBuf>, Error> {
    // Everything is made before anything is written.
    let atlas = document.atlas_with(&options.atlas, limits)?;
    let preview = document.preview_with(
        &PreviewOptions {
            scale: options.scale,
            ..PreviewOptions::default()
        },
        limits,
    )?;
    let files = [
        (ATLAS, png::encode(&atlas.image)),
        (METADATA, atlas.to_json(ATLAS).into_bytes()),
        (PREVIEW, png::encode(&preview)),
    ];
    let io = |path: &Path, e: std::io::Error| {
        Error::new(
            ErrorCode::Io,
            format!("Cannot write {}: {e}", path.display()),
        )
    };
    std::fs::create_dir_all(out).map_err(|e| io(out, e))?;
    let mut written = Vec::new();
    for (name, bytes) in files {
        let path = out.join(name);
        std::fs::write(&path, bytes).map_err(|e| io(&path, e))?;
        written.push(path);
    }
    Ok(written)
}

/// Prints the outcome and returns `status`.
fn report(options: &Options, errors: &[Error], written: &[PathBuf], status: u8) -> ExitCode {
    let file = if options.file == "-" {
        "<stdin>"
    } else {
        options.file.as_str()
    };
    if options.json {
        let mut json = String::new();
        let _ = write!(
            json,
            "{{\"version\":1,\"file\":{},\"ok\":{},\"diagnostics\":[",
            quote(file),
            errors.is_empty()
        );
        for (i, e) in errors.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let _ = write!(json, "{{\"code\":{}", quote(e.code.as_str()));
            if let Some(p) = e.position {
                let _ = write!(json, ",\"line\":{},\"column\":{}", p.line, p.column);
            }
            let _ = write!(json, ",\"message\":{}}}", quote(&e.message));
        }
        json.push_str("],\"written\":[");
        for (i, path) in written.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&quote(&path.display().to_string()));
        }
        json.push_str("]}");
        println!("{json}");
    } else {
        for e in errors {
            match e.position {
                Some(p) => eprintln!(
                    "{file}:{}:{}: error[{}]: {}",
                    p.line, p.column, e.code, e.message
                ),
                None => eprintln!("{file}: error[{}]: {}", e.code, e.message),
            }
        }
        for path in written {
            println!("wrote {}", path.display());
        }
        if errors.is_empty() && options.command == "check" {
            println!("{file}: ok");
        } else if !errors.is_empty() {
            let s = if errors.len() == 1 { "" } else { "s" };
            eprintln!("{file}: {} error{s}", errors.len());
        }
    }
    ExitCode::from(status)
}

/// A JSON string literal.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
