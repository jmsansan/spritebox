//! The `spritebox` command: validate and render spritebox files.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use spritebox::{
    AtlasOptions, Document, Error, ErrorCode, ExportOptions, Limits, Lint, LintCode, LintOptions,
    Position, PreviewOptions, TtfOptions, Warning, png,
};

const USAGE: &str = "\
spritebox: 2D pixel art as plain text

Usage:
  spritebox check <file> [--json]
      Validate a file and print its diagnostics.
  spritebox lint <file> [--strict] [--allow <code>]... [--deny <code>]... [--json]
      Validate a file, then print warnings where its art does not follow
      pixel-art practice. Warnings alone exit 0, unless --strict or --deny.
  spritebox render <file> --out <dir> [--scale <n>] [--padding <n>] [--no-extrude]
                   [--max-side <n>] [--lint] [--theme <name|all>] [--preview] [--json]
      Validate a file, then write atlas.png, tileset-<name>.png for each
      tileset, font-<name>.ttf for each font, spritebox.json (the metadata)
      and preview.png to <dir>.
  spritebox render <file> --screen <name> --out <screen.png> [--scale <n>] [--theme <name>]
      Render one screen to PNG, with integer scale (default 1).
  spritebox font <file> --out <font.ttf> [--font <name>] [--family <name>] [--json]
      Validate a file, then write one of its fonts as TrueType.

<file> may be `-` for standard input.

Options:
  --json        Print diagnostics as JSON on standard output.
  --out <dir>   Directory for output files; created if missing.
  --scale <n>   Pixels per sprite pixel in the preview, 1..=64 (default 4).
  --padding <n> Pixels around each sprite and tile in textures, 0..=64 (default 1).
  --no-extrude  Leave the padding transparent instead of copying edge pixels.
  --lint        Mark lint warnings on the sprites of the preview, with a legend.
  --preview     Explicitly request the preview already included by directory render.
  --screen <n>  Render one screen; --out names a PNG file instead of a directory.
  --theme <n>   Apply a theme to render or lint; render accepts all for shared metadata.
  --max-side <n> The largest side of a texture, 1..=8192 (default 2048).
  --font <name> The font to write, when the file has several.
  --family <n>  The family name in the TrueType file (default: the font's name).
  --strict      Exit 1 when there is any warning.
  --allow <code> Do not report a lint code; may be repeated.
  --deny <code> Exit 1 when a lint code is reported; may be repeated.
  -h, --help    Print this help.
  -V, --version Print the version.

Exit status: 0 success, 1 the file has errors (or, for lint, warnings that
--strict or --deny fail on), 2 bad usage, 3 an I/O error.";

/// Exit statuses, stable for scripts and assistants.
const EXIT_INVALID: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_IO: u8 = 3;

struct Options {
    screen: Option<String>,
    theme: Option<String>,
    command: String,
    file: String,
    out: Option<PathBuf>,
    scale: u32,
    atlas: AtlasOptions,
    font: Option<String>,
    family: Option<String>,
    json: bool,
    /// Mark lint warnings on the preview.
    lint: bool,
    strict: bool,
    allow: Vec<LintCode>,
    deny: Vec<LintCode>,
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
    let mut screen = None;
    let mut theme = None;
    let mut scale_given = false;
    let mut preview = false;
    let command = iter.next().ok_or("missing command")?.clone();
    if !["check", "lint", "render", "font"].contains(&command.as_str()) {
        return Err(format!("unknown command `{command}`"));
    }
    let mut file = None;
    let mut out = None;
    let mut scale = PreviewOptions::default().scale;
    let mut atlas = AtlasOptions::default();
    let mut font = None;
    let mut family = None;
    let mut json = false;
    let mut lint = false;
    let mut strict = false;
    let mut allow = Vec::new();
    let mut deny = Vec::new();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--strict" => strict = true,
            "--allow" | "--deny" => {
                let value = iter.next().ok_or(format!("{arg} needs a lint code"))?;
                let code: LintCode = value.parse().map_err(|e: Error| e.message)?;
                if arg == "--allow" {
                    allow.push(code);
                } else {
                    deny.push(code);
                }
            }
            "--out" => out = Some(PathBuf::from(iter.next().ok_or("--out needs a directory")?)),
            "--scale" => {
                scale_given = true;
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
            "--no-extrude" => atlas.extrude = false,
            "--lint" => lint = true,
            "--preview" => preview = true,
            "--screen" => {
                if screen.is_some() {
                    return Err("--screen may appear only once".into());
                }
                screen = Some(iter.next().ok_or("--screen needs a name")?.clone());
            }
            "--theme" => {
                if theme.is_some() {
                    return Err("--theme may appear only once".into());
                }
                theme = Some(iter.next().ok_or("--theme needs a name or all")?.clone());
            }
            "--max-side" => {
                let value = iter.next().ok_or("--max-side needs a number")?;
                atlas.max_side = value
                    .parse()
                    .ok()
                    .filter(|p| (1..=8192).contains(p))
                    .ok_or(format!("--max-side must be 1..=8192, not `{value}`"))?;
            }
            "--font" => font = Some(iter.next().ok_or("--font needs a name")?.clone()),
            "--family" => family = Some(iter.next().ok_or("--family needs a name")?.clone()),
            _ if arg.starts_with("--") => return Err(format!("unknown option `{arg}`")),
            _ if file.is_none() => file = Some(arg.clone()),
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
    }
    let file = file.ok_or("missing <file>")?;
    if command == "render" && out.is_none() {
        return Err("render needs --out <dir>".into());
    }
    if command == "font" && out.is_none() {
        return Err("font needs --out <font.ttf>".into());
    }
    if (command == "check" || command == "lint") && out.is_some() {
        return Err(format!(
            "{command} writes no files; --out is for render and font"
        ));
    }
    if lint && command != "render" {
        return Err(
            "--lint marks the preview of render; to list warnings, use the lint command".into(),
        );
    }
    if command != "lint" && (strict || !allow.is_empty() || !deny.is_empty()) {
        return Err("--strict, --allow and --deny are for the lint command".into());
    }
    if command != "render" && atlas != AtlasOptions::default() {
        return Err("--padding, --no-extrude and --max-side are for render".into());
    }
    if command != "font" && (font.is_some() || family.is_some()) {
        return Err("--font and --family are for the font command".into());
    }
    if command != "render" && (screen.is_some() || preview) {
        return Err("--screen and --preview are for render".into());
    }
    if theme.is_some() && command != "render" && command != "lint" {
        return Err("--theme is for render or lint".into());
    }
    if theme.as_deref() == Some("all") && (command != "render" || screen.is_some()) {
        return Err("--theme all needs directory rendering without --screen".into());
    }
    if screen.is_some() && !scale_given {
        scale = 1;
    }
    Ok(Options {
        screen,
        theme,
        command,
        file,
        out,
        scale,
        atlas,
        font,
        family,
        json,
        lint,
        strict,
        allow,
        deny,
    })
}

fn run(options: &Options) -> ExitCode {
    let text = match read_input(&options.file) {
        Ok(Ok(text)) => text,
        Ok(Err(error)) => return report(options, &[error], &[], EXIT_INVALID),
        Err(error) => return report(options, &[error], &[], EXIT_IO),
    };
    let limits = Limits::default();
    // Imports are read from disk, relative to the file's directory.
    let base = base_dir(&options.file);
    let mut loader = |name: &str| -> Result<String, Error> {
        let path = import_path(&base, name);
        std::fs::read_to_string(&path)
            .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))
    };
    let errors = spritebox::check_with_loader(&text, &limits, &mut loader);
    if !errors.is_empty() {
        return report(options, &errors, &[], EXIT_INVALID);
    }
    if options.command == "check" {
        return report(options, &[], &[], 0);
    }
    let mut document = match Document::parse_with_loader(&text, &limits, &mut loader) {
        Ok(document) => document,
        Err(error) => return report(options, &[error], &[], EXIT_INVALID),
    };
    if let Some(theme) = options.theme.as_deref().filter(|name| *name != "all") {
        document = match document.with_theme(theme) {
            Ok(doc) => doc,
            Err(e) => return report(options, &[e], &[], EXIT_INVALID),
        };
    }
    if options.command == "lint" {
        let lint = document.lint_with(
            &LintOptions {
                screen_overlap: options.deny.contains(&LintCode::ScreenOverlap),
                allow: options.allow.clone(),
                ..LintOptions::default()
            },
            &limits,
        );
        // Counts include warnings beyond the limit of those printed.
        let fails = (options.strict && !lint.is_clean())
            || options.deny.iter().any(|&code| lint.count(code) > 0);
        let status = if fails { EXIT_INVALID } else { 0 };
        return report_lint(options, &lint, status);
    }
    let out = options.out.as_deref().unwrap_or(Path::new("."));
    let result = if options.command == "font" {
        write_font(&document, out, options)
    } else {
        render(&document, out, options, &limits)
    };
    match result {
        Ok(written) => report(options, &[], &written, 0),
        Err(error) => {
            let status = match error.code {
                ErrorCode::Io => EXIT_IO,
                ErrorCode::InvalidArgument if options.command == "font" => EXIT_USAGE,
                _ => EXIT_INVALID,
            };
            report(options, &[error], &[], status)
        }
    }
}

/// Writes the chosen font to `out`. A missing or ambiguous font is an
/// `InvalidArgument` error, reported with the usage exit status.
fn write_font(document: &Document, out: &Path, options: &Options) -> Result<Vec<PathBuf>, Error> {
    let fonts = document.fonts();
    let font = match &options.font {
        Some(name) => document.font(name).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidArgument,
                format!("The file has no font `{name}`"),
            )
        })?,
        None if fonts.len() == 1 => &fonts[0],
        None => {
            return Err(Error::new(
                ErrorCode::InvalidArgument,
                format!(
                    "The file defines {} fonts; choose one with --font <name>",
                    fonts.len()
                ),
            ));
        }
    };
    let ttf = font.to_ttf(&TtfOptions {
        family: options.family.clone(),
        ..TtfOptions::default()
    })?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::new(
                ErrorCode::Io,
                format!("Cannot write {}: {e}", parent.display()),
            )
        })?;
    }
    std::fs::write(out, ttf).map_err(|e| {
        Error::new(
            ErrorCode::Io,
            format!("Cannot write {}: {e}", out.display()),
        )
    })?;
    Ok(vec![out.to_path_buf()])
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

fn render(
    document: &Document,
    out: &Path,
    options: &Options,
    limits: &Limits,
) -> Result<Vec<PathBuf>, Error> {
    if let Some(name) = &options.screen {
        let image = document
            .screen_with(name, limits)?
            .ok_or_else(|| Error::new(ErrorCode::UnknownName, format!("Unknown screen `{name}`")))?
            .scaled_with(options.scale, limits)?;
        let bytes = png::encode(&image);
        if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out, bytes)?;
        return Ok(vec![out.to_path_buf()]);
    }
    // Everything is made before anything is written.
    let export_options = ExportOptions {
        atlas: options.atlas.clone(),
    };
    let export = if options.theme.as_deref() == Some("all") {
        document.export_all_themes_with(&export_options, limits)?
    } else {
        document.export_with(&export_options, limits)?
    };
    let preview = document.preview_with(
        &PreviewOptions {
            scale: options.scale,
            lint: options.lint,
            ..PreviewOptions::default()
        },
        limits,
    )?;
    let mut files = export.files();
    files.push(("preview.png".to_owned(), png::encode(&preview)));
    let mut names = std::collections::HashSet::new();
    if files.len() > limits.max_export_files {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            "More than the export file limit",
        ));
    }
    for (name, _) in &files {
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(Error::new(
                ErrorCode::InvalidArgument,
                format!("Output filenames collide at `{name}`"),
            ));
        }
    }
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

/// The directory that imports resolve from: the file's, or the current one for
/// standard input.
fn base_dir(file: &str) -> PathBuf {
    if file == "-" {
        return PathBuf::new();
    }
    Path::new(file)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// The path of an imported file: its `/`-separated name under `base`, with the
/// platform's separators.
fn import_path(base: &Path, name: &str) -> PathBuf {
    let mut path = if name.starts_with('/') {
        PathBuf::from("/")
    } else {
        base.to_path_buf()
    };
    path.extend(name.split('/').filter(|part| !part.is_empty()));
    path
}

/// The file a diagnostic is in, as the user would open it.
fn diagnostic_file(options: &Options, file: &Option<String>, main: &str) -> String {
    match file {
        Some(imported) => import_path(&base_dir(&options.file), imported)
            .display()
            .to_string(),
        None => main.to_owned(),
    }
}

/// An error or a warning, as printed.
struct Diagnostic<'d> {
    severity: &'static str,
    code: &'static str,
    file: &'d Option<String>,
    position: Option<Position>,
    message: &'d str,
}

/// Prints lint's warnings and returns `status`.
fn report_lint(options: &Options, lint: &Lint, status: u8) -> ExitCode {
    print(options, &[], &lint.warnings, lint.omitted(), &[], status)
}

/// Prints errors and written files, and returns `status`.
fn report(options: &Options, errors: &[Error], written: &[PathBuf], status: u8) -> ExitCode {
    print(options, errors, &[], 0, written, status)
}

/// Prints the outcome and returns `status`. `omitted` warnings were left out by the
/// limit.
fn print(
    options: &Options,
    errors: &[Error],
    warnings: &[Warning],
    omitted: usize,
    written: &[PathBuf],
    status: u8,
) -> ExitCode {
    let file = if options.file == "-" {
        "<stdin>"
    } else {
        options.file.as_str()
    };
    let diagnostics: Vec<Diagnostic> = errors
        .iter()
        .map(|e| Diagnostic {
            severity: "error",
            code: e.code.as_str(),
            file: &e.file,
            position: e.position,
            message: &e.message,
        })
        .chain(warnings.iter().map(|w| Diagnostic {
            severity: "warning",
            code: w.code.as_str(),
            file: &w.file,
            position: w.position,
            message: &w.message,
        }))
        .collect();
    if options.json {
        let mut json = String::new();
        let _ = write!(
            json,
            "{{\"version\":1,\"file\":{},\"ok\":{},\"diagnostics\":[",
            quote(file),
            status == 0
        );
        for (i, d) in diagnostics.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let _ = write!(
                json,
                "{{\"code\":{},\"severity\":{}",
                quote(d.code),
                quote(d.severity)
            );
            if d.file.is_some() {
                let _ = write!(
                    json,
                    ",\"file\":{}",
                    quote(&diagnostic_file(options, d.file, file))
                );
            }
            if let Some(p) = d.position {
                let _ = write!(json, ",\"line\":{},\"column\":{}", p.line, p.column);
            }
            let _ = write!(json, ",\"message\":{}}}", quote(d.message));
        }
        json.push(']');
        if options.command == "lint" {
            let _ = write!(json, ",\"omitted\":{omitted}");
        }
        json.push_str(",\"written\":[");
        for (i, path) in written.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&quote(&path.display().to_string()));
        }
        json.push_str("]}");
        println!("{json}");
    } else {
        for d in &diagnostics {
            let at = diagnostic_file(options, d.file, file);
            match d.position {
                Some(p) => eprintln!(
                    "{at}:{}:{}: {}[{}]: {}",
                    p.line, p.column, d.severity, d.code, d.message
                ),
                None => eprintln!("{at}: {}[{}]: {}", d.severity, d.code, d.message),
            }
        }
        for path in written {
            println!("wrote {}", path.display());
        }
        let count = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
        if !errors.is_empty() {
            eprintln!("{file}: {}", count(errors.len(), "error"));
        } else if omitted > 0 {
            let total = warnings.len() + omitted;
            eprintln!(
                "{file}: {}, of which the first {} are shown",
                count(total, "warning"),
                warnings.len()
            );
        } else if !warnings.is_empty() {
            eprintln!("{file}: {}", count(warnings.len(), "warning"));
        } else if options.command == "check" || options.command == "lint" {
            println!("{file}: ok");
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
