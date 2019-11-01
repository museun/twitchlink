use gumdrop::Options;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("cannot get access token for '{channel}' because: {source}")]
    GetAccessToken {
        channel: String,
        source: attohttpc::Error,
    },

    #[error("cannot deserialize response for '{channel}' because: {source}")]
    Deserialize {
        channel: String,
        source: attohttpc::Error,
    },

    #[error("cannot get playlist for '{channel}' because: {source}")]
    GetPlaylist {
        channel: String,
        source: attohttpc::Error,
    },

    #[error("cannot get response body for '{channel}' because: {source}")]
    GetResponseBody {
        channel: String,
        source: attohttpc::Error,
    },

    #[error("cannot playlist for '{channel}'")]
    InvalidPlaylist { channel: String },

    #[error("cannot find token for '{channel}'")]
    FindToken { channel: String },

    #[error("cannot find signature for '{channel}'")]
    FindSignature { channel: String },
}

struct Client {
    client_id: String,
}

impl Client {
    fn new(id: impl ToString) -> Self {
        Self {
            client_id: id.to_string(),
        }
    }

    fn get(&self, channel: impl AsRef<str>) -> Result<Vec<Stream>, Error> {
        let channel = channel.as_ref();
        let playlist = self.fetch_playlist(channel)?;

        let mut map = HashMap::new();

        // why
        let (mut quality, mut resolution, mut bandwidth) =
            (String::new(), String::new(), String::new());

        for line in playlist.lines() {
            if line.contains("VIDEO=") {
                let (index, _) =
                    line.match_indices("VIDEO=")
                        .next()
                        .ok_or_else(|| Error::InvalidPlaylist {
                            channel: channel.to_string(),
                        })?;

                quality = line[index + "VIDEO=".len()..].replace("\"", "");

                let search = |q: &str| {
                    let pos = line.find(q).unwrap();
                    let end = (&line[pos..].find(',')).unwrap() + pos;
                    &line[pos + q.len()..end]
                };

                bandwidth = search("BANDWIDTH=").to_string();
                resolution = search("RESOLUTION=").to_string();
            }

            if quality.is_empty() || line.starts_with('#') {
                continue;
            }

            let s = match (quality.as_str(), quality[..3].parse::<u32>()) {
                ("chunked", ..) => Stream {
                    link: line.to_string(),
                    resolution: std::mem::replace(&mut resolution, String::new()),
                    bandwidth: std::mem::replace(&mut bandwidth, String::new()),
                    quality: None,
                    ty: "best".into(),
                },
                (.., Ok(n)) => Stream {
                    link: line.to_string(),
                    resolution: std::mem::replace(&mut resolution, String::new()),
                    bandwidth: std::mem::replace(&mut bandwidth, String::new()),
                    quality: Some(n),
                    ty: format!("{}p", n),
                },
                (s, ..) => {
                    eprintln!("WARN: unknown quality: {}", s);
                    quality.clear();
                    continue;
                }
            };

            map.insert(s.quality, s);
            quality.clear();
        }

        let mut list = map.drain().map(|(_, v)| v).collect::<Vec<_>>();
        list.sort_unstable_by(|a, b| match (a.quality, b.quality) {
            (Some(a), Some(b)) => b.cmp(&a),
            (None, ..) => std::cmp::Ordering::Less,
            (.., None) => std::cmp::Ordering::Greater,
        });
        Ok(list)
    }

    fn fetch_playlist(&self, channel: &str) -> Result<String, Error> {
        let val: serde_json::Value = attohttpc::get(format!(
            "https://api.twitch.tv/api/channels/{}/access_token",
            channel
        ))
        .header("Client-ID", self.client_id.clone())
        .send()
        .map_err(|err| Error::GetAccessToken {
            channel: channel.to_string(),
            source: err,
        })?
        .json()
        .map_err(|err| Error::Deserialize {
            channel: channel.to_string(),
            source: err,
        })?;

        let (token, sig) = match (
            val.get("token").and_then(serde_json::Value::as_str),
            val.get("sig").and_then(serde_json::Value::as_str),
        ) {
            (Some(token), Some(sig)) => (token, sig),
            (None, ..) => {
                return Err(Error::FindToken {
                    channel: channel.to_string(),
                })
            }
            (.., None) => {
                return Err(Error::FindSignature {
                    channel: channel.to_string(),
                })
            }
        };

        attohttpc::get(format!(
            "https://usher.ttvnw.net/api/channel/hls/{}.m3u8",
            channel,
        ))
        .params(&[
            ("token", token),
            ("sig", sig),
            ("player_backend", "html5"),
            ("player", "twitchweb"),
            ("type", "any"),
            ("allow_source", "true"),
        ])
        .send()
        .map_err(|err| Error::GetPlaylist {
            channel: channel.to_string(),
            source: err,
        })?
        .text()
        .map_err(|err| Error::GetResponseBody {
            channel: channel.to_string(),
            source: err,
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, PartialOrd, Eq, Ord)]
struct Stream {
    resolution: String,
    bandwidth: String,
    link: String,
    #[serde(skip)]
    quality: Option<u32>,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Quality {
    Best,
    Lowest,
    Custom(String),
}

impl std::str::FromStr for Quality {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let input = s.to_ascii_lowercase();
        let ok = match input.as_str() {
            "best" | "highest" => Quality::Best,
            "worst" | "lowest " => Quality::Lowest,
            _ => Quality::Custom(input), // try parsing this maybe
        };
        Ok(ok)
    }
}

#[derive(Serialize)]
struct Item {
    quality: String,
    resolution: String,
    bitrate: String,
}

impl From<Stream> for Item {
    fn from(s: Stream) -> Self {
        Item {
            quality: s.ty,
            resolution: s.resolution,
            bitrate: s.bandwidth,
        }
    }
}

impl std::fmt::Display for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {: >10} @ {: >8.2} kbps",
            self.quality,
            self.resolution,
            self.bitrate.parse::<f64>().unwrap() / 1024.
        )
    }
}

trait Abort<T, E = ()> {
    fn abort<F: FnOnce(E) -> String>(self, f: F) -> T;
}

impl<T, E: std::fmt::Display> Abort<T, E> for Result<T, E> {
    fn abort<F: FnOnce(E) -> String>(self, f: F) -> T {
        self.unwrap_or_else(|err| {
            eprintln!("{}", f(err));
            std::process::exit(1);
        })
    }
}
impl<T> Abort<T, ()> for Option<T> {
    fn abort<F: FnOnce(()) -> String>(self, f: F) -> T {
        self.unwrap_or_else(|| {
            eprintln!("{}", f(()));
            std::process::exit(1);
        })
    }
}

#[derive(Options, Debug, Clone)]
struct Args {
    #[options(help = "display this message")]
    help: bool,

    #[options(help = "dumps the stream information as json")]
    json: bool,

    #[options(help = "a player to use.")]
    player: Option<String>,

    #[options(help = "desired quality of the stream")]
    quality: Option<Quality>,

    #[options(help = "list stream quality information")]
    list: bool,

    #[options(required, free, help = "the stream to fetch")]
    stream: String,
}

fn main() {
    let player = std::env::var("STREAMLINK_PLAYER")
        .ok()
        .unwrap_or_else(|| "mpv".to_string());

    // TODO show the version
    let args = Args::parse_args_default_or_exit();

    let player = args.player.unwrap_or_else(|| player.to_string());
    if std::fs::metadata(&player).is_err() {
        eprintln!("error: invalid path: {}. set `STREAMLINK_PLAYER` or provide a path to a valid executable", player);
        std::process::exit(1);
    }

    let channel = if args.stream.contains('/') {
        args.stream.split('/').last().unwrap()
    } else {
        args.stream.as_str()
    };

    let id = std::env::var("TWITCH_CLIENT_ID")
        .abort(|_| "env. var 'TWITCH_CLIENT_ID' must be set to your client id".to_string());

    let client = Client::new(id);
    let streams = client.get(&channel).abort(|err| err.to_string());

    let singular = args.quality.is_some();

    let quality = args.quality.unwrap_or_else(|| Quality::Best);
    let stream = match quality {
        Quality::Best => streams
            .first()
            .abort(|_| format!("stream `{}` is offline", channel)),

        Quality::Lowest => streams
            .last()
            .abort(|_| format!("stream `{}` is offline", channel)),

        Quality::Custom(mut s) => {
            if !s.ends_with('p') {
                s.push('p');
            }
            streams
                .iter()
                .find(|stream| stream.ty == *s)
                .abort(|_| format!("quality `{}` is not available for stream `{}` ", s, channel))
        }
    };

    if args.json && !args.list {
        let s = if !singular {
            serde_json::to_string(&streams)
        } else {
            serde_json::to_string(&stream)
        }
        .unwrap();

        println!("{}", s);
        return;
    }

    match (args.json, args.list, singular) {
        (false, true, false) => streams
            .into_iter()
            .map(Item::from)
            .for_each(|k| println!("{}", k)),

        (false, true, true) => println!("{}", Item::from(stream.clone())),

        (true, true, true) => println!(
            "{}",
            serde_json::to_string(&Item::from(stream.clone())).unwrap()
        ),

        (true, true, false) => println!(
            "{}",
            serde_json::to_string(
                &streams.into_iter().map(Item::from).collect::<Vec<_>>() //
            )
            .unwrap()
        ),

        _ => std::process::Command::new(&player)
            .arg(&stream.link)
            .spawn()
            .map(|_| ())
            .abort(|err| {
                format!(
                    "cannot start stream `{}`. make sure `{}` is a valid player\nerror: {}",
                    channel, player, err
                )
            }),
    }
}
