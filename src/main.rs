#[macro_use]
extern crate failure;
extern crate curl;
extern crate serde;
extern crate serde_json;

use curl::easy::{Easy, List};
use failure::Error;

use std::{env, process, process::Command};

const CLIENT_ID: &str = env!("TWITCH_CLIENTID");
const PLAYER: &str = r#"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe"#;

fn parse_access_token(val: &serde_json::Value) -> Option<(&str, &str)> {
    let token = match val.get("token") {
        Some(token) if token.is_string() => token.as_str(),
        _ => None,
    };

    let sig = match val.get("sig") {
        Some(sig) if sig.is_string() => sig.as_str(),
        _ => None,
    };

    if token.is_some() && sig.is_some() {
        return Some((token.unwrap(), sig.unwrap()));
    }
    None
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
            .unwrap();

        transfer.perform().unwrap();
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
    let json = serde_json::from_slice(&resp)?;
    let at = parse_access_token(&json).ok_or_else(|| format_err!("cannot get access token"))?;

    easy.url(&format!(
        "https://usher.ttvnw.net/api/channel/hls/{}.m3u8{}",
        channel,
        build_query(at.0, at.1)
    )).unwrap();

    let resp = get_response(&mut easy)?;
    Ok(String::from_utf8(resp)?)
}

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

fn run() -> Result<(), failure::Error> {
    let channel = {
        let mut args = env::args();
        if args.len() != 2 {
            eprintln!("usage: {} <name>", args.nth(0).unwrap());
            process::exit(-1);
        }
        let mut ch = args.nth(1).unwrap();
        if ch.contains('/') {
            ch = ch.split('/').last().unwrap().to_string();
        }
        ch
    };

    if let Some(stream) = get_stream(get_playlist(&channel)?.as_str()) {
        if let Err(err) = Command::new(PLAYER).arg(&stream).spawn() {
            return Err(format_err!("cannot start player: {}", err));
        }
        Ok(())
    } else {
        Err(format_err!("{} is offline", channel))
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("failed: {}", e);
        process::exit(-1);
    }
}
