extern crate curl;
extern crate getopts;
extern crate json;

use curl::easy::{Easy, List};
use getopts::Options;
use std::{env, fmt, process, process::Command};

const CLIENT_ID: &str = env!("TWITCH_CLIENTID");

enum Error {
    CannotGetResponse(curl::Error),
    CannotParseJson(json::Error),
    CannotParseAccessToken,
    CannotParseResponse(std::string::FromUtf8Error),
    CannotStartPlayer(std::io::Error),
    IsOffline(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Error::CannotGetResponse(e) => write!(f, "cannot get response: {}", e),
            Error::CannotParseJson(e) => write!(f, "cannot parse json: {}", e),
            Error::CannotParseAccessToken => write!(f, "cannot parse access token"),
            Error::CannotParseResponse(e) => write!(f, "cannot parse response: {}", e),
            Error::CannotStartPlayer(e) => write!(f, "cannot start player: {}", e),
            Error::IsOffline(ch) => write!(f, "{} is offline", ch),
        }
    }
}

fn parse_access_token(val: &json::JsonValue) -> Option<(&str, &str)> {
    if !val["token"].is_string() {
        return None;
    }

    if !val["sig"].is_string() {
        return None;
    }

    Some((val["token"].as_str().unwrap(), val["sig"].as_str().unwrap()))
}

fn build_query(token: &str, sig: &str) -> String {
    fn encode(data: &str) -> String {
        let mut res = String::new();
        for ch in data.as_bytes().iter() {
            match *ch as char {
                'A'...'Z' | 'a'...'z' | '0'...'9' | '-' | '_' | '.' | '~' => res.push(*ch as char),
                ch => res.push_str(format!("%{:02X}", ch as u32).as_str()),
            }
        }
        res
    }

    let map = &[
        ("token", token),
        ("sig", sig),
        ("player_backend", "html5"),
        ("player", "twitchweb"),
        ("type", "any"),
        ("allow_source", "true"),
    ];

    let mut query = String::from("?");
    for (k, v) in map {
        query.push_str(&format!("{}={}&", encode(k), encode(v),));
    }
    query
}

fn get_response(easy: &mut curl::easy::Easy) -> Result<Vec<u8>, Error> {
    let mut resp = vec![];
    {
        let mut transfer = easy.transfer();
        transfer
            .write_function(|data| {
                resp.extend_from_slice(data);
                Ok(data.len())
            })
            .map_err(Error::CannotGetResponse)?;
        transfer.perform().map_err(Error::CannotGetResponse)?;
    }
    Ok(resp)
}

fn get_playlist(channel: &str) -> Result<String, Error> {
    let mut easy = Easy::new();
    let mut list = List::new();
    list.append(&format!("Client-ID: {}", CLIENT_ID)).unwrap();
    easy.http_headers(list).unwrap();
    easy.url(&format!(
        "https://api.twitch.tv/api/channels/{}/access_token",
        channel
    )).unwrap();

    let resp = get_response(&mut easy)?;
    let resp = String::from_utf8(resp).map_err(Error::CannotParseResponse)?;
    let json = json::parse(&resp).map_err(Error::CannotParseJson)?;
    let at = parse_access_token(&json).ok_or_else(|| Error::CannotParseAccessToken)?;

    easy.url(&format!(
        "https://usher.ttvnw.net/api/channel/hls/{}.m3u8{}",
        channel,
        build_query(at.0, at.1)
    )).unwrap();

    let resp = get_response(&mut easy)?;
    Ok(String::from_utf8(resp).map_err(Error::CannotParseResponse)?)
}

struct Stream {
    link: String,
    quality: String,
}

// this needs to return a list of stream qualities
fn get_stream(playlist: &str) -> Option<&str> {
    static PREFIX: &str = "VIDEO=";

    use std::collections::BTreeMap;
    let mut map = BTreeMap::new();

    // TODO use an enum for this. probably.
    let mut quality = String::new();

    for line in playlist.lines() {
        if line.contains(PREFIX) {
            let &(index, _) = line.match_indices(PREFIX).collect::<Vec<_>>().first()?;
            let offset = index + PREFIX.len();
            quality = line[offset..].replace("\"", "");
        }
        if quality.is_empty() || line.starts_with('#') {
            continue;
        }
        if quality == "chunked" {
            map.insert(9999, line); // this is the source quality
        } else if let Ok(q) = quality[..3].parse::<i32>() {
            map.insert(q, line);
        }
        quality.clear();
    }

    // get the last element, so the largest number
    if let Some((_, v)) = map.iter().rev().next() {
        return Some(v);
    }
    None
}

fn print_usage(program: &str, opts: &Options) {
    use std::process;
    let brief = format!("usage: {} stream", program);
    print!("{}", opts.usage(&brief));
    process::exit(1);
}

fn run(json: bool, player: &str, channel: &str) -> Result<(), Error> {
    if let Some(stream) = get_stream(get_playlist(&channel)?.as_str()) {
        if json {
            Ok(())?
        }
        if let Err(err) = Command::new(player).arg(&stream).spawn() {
            Err(Error::CannotStartPlayer(err))?;
        }
        Ok(())
    } else {
        Err(Error::IsOffline(channel.to_string()))
    }
}

fn main() {
    const PLAYER: &str = r#"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe"#;

    let args = env::args().collect::<Vec<_>>();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("j", "json", "dumps stream link as json");
    opts.optflag("h", "help", "prints this help menu");
    opts.optopt(
        "p",
        "player",
        "the player to use (assumes the player accepts the stream on stdin)",
        "PLAYER",
    );

    let matches = opts.parse(&args[1..]).unwrap();
    if matches.opt_present("h") {
        print_usage(&program, &opts)
    }

    let json = matches.opt_present("j");
    let player = matches.opt_get_default("p", PLAYER.to_string()).unwrap();

    let channel = if !matches.free.is_empty() {
        let mut ch = matches.free[0].clone();
        if ch.contains('/') {
            ch = ch.split('/').last().unwrap().to_string();
        }
        ch
    } else {
        print_usage(&program, &opts);
        return;
    };

    if let Err(e) = run(json, &player, &channel) {
        eprintln!("failed: {}", e);
        process::exit(-1);
    }
}
