use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::Parser;
use reqwest::blocking::Client;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value = "default")]
    firefox_profile: OsString,
    #[arg(short, long, default_value = "./inputs")]
    target_directory: PathBuf,
}

type Result<T> = std::result::Result<T, eyre::Report>;

fn main() -> Result<()> {
    let Args {
        firefox_profile,
        target_directory,
    } = Args::parse();

    let session_cookie = get_session_cookie(&firefox_profile)?;

    let client = Client::new();

    if !target_directory.is_dir() {
        fs::create_dir(&target_directory)?;
    }

    for day in 1..31 {
        let exists = fetch_input(&client, &session_cookie, &target_directory, day)?;
        if !exists {
            break;
        }
    }

    Ok(())
}

fn get_session_cookie(firefox_profile: &OsStr) -> Result<String> {
    let mut profile_dir = None;
    let firefox_dir = home::home_dir()
        .ok_or_else(|| eyre::eyre!("idk where your home dir is"))?
        .join(".mozilla/firefox");
    for r in fs::read_dir(&firefox_dir)? {
        let entry = r?;
        if !entry.metadata()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if path.extension() == Some(firefox_profile) {
            profile_dir = Some(path);
            break;
        }
    }

    let profile_dir = profile_dir.ok_or_else(|| {
        eyre::eyre!(format!(
            "couldn't find firefox profile dir in {}",
            firefox_dir.display()
        ))
    })?;

    let cookies_path = profile_dir.join("cookies.sqlite");

    let cookies_path = cookies_path.to_str()
        .ok_or_else(|| eyre::eyre!("your firefox dir should probably have a utf-8 path"))?;

    let db = rusqlite::Connection::open_with_flags(
        &format!("file:{cookies_path}?immutable=1"), // todo url encode i guess
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
        | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;

    const QUERY: &str = r"
        select value
        from moz_cookies
        where host = '.adventofcode.com'
          and path = '/'
          and name = 'session'
    ";
    let cookie = db.query_row(QUERY, [], |row| row.get::<_, String>(0))?;

    return Ok(format!("session={cookie}"));
}

fn fetch_input(
    client: &Client,
    session_cookie: &str,
    target_directory: &Path,
    day: i32,
) -> Result<bool> {
    let dest_path = target_directory.join(&format!("day-{day:02}-input.txt"));

    if let Ok(m) = fs::metadata(&dest_path) {
        if m.is_file() {
            if m.len() == 0 {
                eprintln!("deleting empty file {}", dest_path.display());
                fs::remove_file(&dest_path)?;
            } else {
                eprintln!("already got non-empty file {}, skipping day {day}", dest_path.display());
                return Ok(true);
            }
        }
    }
    use reqwest::header;
    use reqwest::StatusCode;

    let mut response = client
        .request(
            reqwest::Method::GET,
            format!("https://adventofcode.com/2023/day/{day}/input"),
        )
        .header(header::COOKIE, session_cookie)
        .header(header::USER_AGENT, "aoc fetch-inputs (advent-of-code-fetch-inputs@1d6.org)")
        .send()?;

    if response.status() == StatusCode::NOT_FOUND {
        eprintln!("input for day {day} not found, try again tomorrow or w/e");
        return Ok(false);
    } else if !response.status().is_success() {
        eprintln!("unsuccessful response for day {day}");
        response.copy_to(&mut io::stdout())?;
    }

    let mut response = response.error_for_status()?;

    let mut output = {
        let r = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest_path);
        match r {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                eprintln!("non-empty file {} just showed up, skipping day {day}", dest_path.display());
                return Ok(true);
            }
            Err(e) => Err(e)?,
            Ok(r) => r,
        }
    };

    response.copy_to(&mut output)?;

    eprintln!("day {day}: wrote {}", dest_path.display());
    Ok(true)
}
