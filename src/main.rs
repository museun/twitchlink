use getopts::Options;
use serde::Serialize;
use std::env;

#[derive(Debug, Serialize, PartialEq, PartialOrd, Eq, Ord)]
struct Stream {
    resolution: String,
    bandwidth: String,
    link: String,
    #[serde(skip)]
    quality: i32,
    #[serde(rename = "type")]
    ty: String,
}

impl Stream {
    pub fn get(channel: &str) -> Result<Vec<Self>, String> {
        let playlist = Self::playlist(channel)?;

        static VIDEO: &str = "VIDEO=";
        static RESOLUTION: &str = "RESOLUTION=";
        static BANDWIDTH: &str = "BANDWIDTH=";

        let mut map = std::collections::HashMap::new();

        // TODO look at this. this shouldn't be done like this

        let mut quality = String::new();
        let mut resolution = String::new();
        let mut bandwidth = String::new();

        for line in playlist.lines() {
            if line.contains(VIDEO) {
                // #EXT-X-STREAM-INF:PROGRAM-ID=1,BANDWIDTH=6874872,RESOLUTION=1920x1080,CODECS="avc1.4D402A,mp4a.40.2",VIDEO="chunked"
                let (index, _) = line
                    .match_indices(VIDEO)
                    .next()
                    .ok_or_else(|| format!("cannot parse playlist for `{}`", channel))?;

                let offset = index + VIDEO.len();
                quality = line[offset..].replace("\"", "");

                {
                    let pos = line.find(BANDWIDTH).unwrap();
                    let end = (&line[pos..].find(',')).unwrap() + pos;
                    bandwidth = line[pos + BANDWIDTH.len()..end].to_string();
                }

                {
                    let pos = line.find(RESOLUTION).unwrap();
                    let end = (&line[pos..].find(',')).unwrap() + pos;
                    resolution = line[pos + RESOLUTION.len()..end].to_string();
                }
            }
            if quality.is_empty() || line.starts_with('#') {
                continue;
            }
            if quality == "chunked" {
                map.insert(
                    9999,
                    Stream {
                        link: line.to_string(),
                        resolution: resolution.to_string(),
                        bandwidth: bandwidth.to_string(),
                        quality: 9999,
                        ty: "best".into(),
                    },
                ); // this is the source quality
            } else if let Ok(q) = quality[..3].parse::<i32>() {
                map.insert(
                    q,
                    Stream {
                        link: line.to_string(),
                        resolution: resolution.to_string(),
                        bandwidth: bandwidth.to_string(),
                        quality: q,
                        ty: format!("{}p", q),
                    },
                );
            }

            // what is this doing?
            // not sure if we can drain instead, this might be partial data
            resolution.clear();
            bandwidth.clear();
            quality.clear();
        }

        let mut list = map.drain().map(|(_, v)| v).collect::<Vec<_>>();
        list.sort_unstable_by(|a, b| b.quality.cmp(&a.quality));
        Ok(list)
    }

    fn playlist(channel: &str) -> Result<String, String> {
        let resp = ureq::get(&format!(
            "https://api.twitch.tv/api/channels/{}/access_token",
            channel
        ))
        .set(
            "Client-ID",
            &env::var("TWITCH_CLIENT_ID").map_err(|_| "TWITCH_CLIENT_ID is not set")?,
        )
        .call();

        if let Some(err) = resp.synthetic_error() {
            return Err(err.status_text().into());
        }

        let val: serde_json::Value =
            serde_json::from_reader(resp.into_reader()).map_err(|err| err.to_string())?;

        let (token, sig) = match (
            val.get("token").and_then(serde_json::Value::as_str),
            val.get("sig").and_then(serde_json::Value::as_str),
        ) {
            (Some(token), Some(sig)) => (token, sig),
            (None, ..) => return Err(format!("cannot get token for: {}", channel)),
            (.., None) => return Err(format!("cannot get sig for: {}", channel)),
        };

        let mut req = ureq::get(&format!(
            "https://usher.ttvnw.net/api/channel/hls/{}.m3u8",
            channel,
        ));

        for (k, v) in &[
            ("token", token),
            ("sig", sig),
            ("player_backend", "html5"),
            ("player", "twitchweb"),
            ("type", "any"),
            ("allow_source", "true"),
        ] {
            req = req.query(k, v).build()
        }

        let resp = req.call();
        if let Some(err) = resp.synthetic_error() {
            return Err(err.status_text().into());
        }

        resp.into_string().map_err(|err| err.to_string())
    }
}

fn main() -> Result<(), String> {
    const PLAYER: &str = r#"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe"#;

    let (program, args) = {
        let mut args = env::args();
        (args.next().unwrap(), args.collect::<Vec<_>>())
    };

    let mut opts = Options::new();
    opts.optflag("j", "json", "dumps stream link as json");
    opts.optflag("h", "help", "prints this help menu");
    opts.optopt(
        "p",
        "player",
        "the player to use (assumes the player accepts the stream on stdin)",
        "PLAYER",
    );

    let matches = opts.parse(&args).unwrap();
    if matches.opt_present("h") || matches.free.is_empty() {
        print_usage(&program, &opts)
    }

    let json = matches.opt_present("j");
    let player = matches.opt_get_default("p", PLAYER.to_owned()).unwrap();

    let channel = &matches.free[0];
    let channel = if channel.contains('/') {
        channel.split('/').last().unwrap()
    } else {
        channel
    };

    let streams = Stream::get(&channel)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&streams).unwrap());
        return Ok(());
    }

    let stream = streams
        .last()
        .ok_or_else(|| format!("stream `{}` is offline", channel))?;

    std::process::Command::new(player)
        .arg(&stream.link)
        .spawn()
        .map_err(|err| format!("cannot start stream `{}`: {}", channel, err))
        .map(|_| ())
}

fn print_usage(program: &str, opts: &Options) -> ! {
    print!("{}", opts.usage(&format!("usage: {} stream", program)));
    std::process::exit(1);
}
