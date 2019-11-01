use crate::error::Error;
use serde::Serialize;
use std::collections::HashMap;

pub fn get(client_id: impl AsRef<str>, channel: impl AsRef<str>) -> Result<Vec<Stream>, Error> {
    let (client_id, channel) = (client_id.as_ref(), channel.as_ref());
    let playlist = fetch_playlist(client_id, channel)?;

    let mut map = HashMap::new();

    // why
    let (mut quality, mut resolution, mut bandwidth) =
        (String::new(), String::new(), String::new());

    for line in playlist.lines() {
        if line.contains("VIDEO=") {
            let (index, _) = line
                .match_indices("VIDEO=")
                .next()
                .ok_or_else(|| Error::InvalidPlaylist)?;

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

        use std::mem::replace;

        let s = match (quality.as_str(), quality[..3].parse::<u32>()) {
            ("chunked", _) => Stream {
                link: line.to_string(),
                resolution: replace(&mut resolution, String::new()),
                bandwidth: replace(&mut bandwidth, String::new()),
                quality: None,
                ty: "best".into(),
            },
            (_, Ok(n)) => Stream {
                link: line.to_string(),
                resolution: replace(&mut resolution, String::new()),
                bandwidth: replace(&mut bandwidth, String::new()),
                quality: Some(n),
                ty: format!("{}p", n),
            },
            (s, _) => {
                eprintln!("WARN: unknown quality: {}", s);
                quality.clear();
                continue;
            }
        };

        map.insert(s.quality, s);
        quality.clear();
    }

    use std::cmp::Ordering::*;

    let mut list = map.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
    list.sort_unstable_by(|a, b| match (a.quality, b.quality) {
        (Some(a), Some(b)) => b.cmp(&a),
        (None, _) => Less,
        (_, None) => Greater,
    });
    Ok(list)
}

pub fn fetch_playlist(client_id: &str, channel: &str) -> Result<String, Error> {
    let val: serde_json::Value = attohttpc::get(format!(
        "https://api.twitch.tv/api/channels/{}/access_token",
        channel
    ))
    .header("Client-ID", client_id)
    .send()
    .map_err(Error::GetAccessToken)?
    .json()
    .map_err(Error::Deserialize)?;

    let (token, sig) = match (
        val.get("token").and_then(serde_json::Value::as_str),
        val.get("sig").and_then(serde_json::Value::as_str),
    ) {
        (Some(token), Some(sig)) => (token, sig),
        (None, _) => return Err(Error::FindToken),
        (_, None) => return Err(Error::FindSignature),
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
    .map_err(Error::GetPlaylist)?
    .text()
    .map_err(Error::GetResponseBody)
}

#[derive(Debug, Clone, Serialize, PartialEq, PartialOrd, Eq, Ord)]
pub struct Stream {
    pub resolution: String,
    pub bandwidth: String,
    pub link: String,
    #[serde(skip)]
    pub quality: Option<u32>,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Quality {
    Best,
    Worst,
    Custom(String),
}

impl std::str::FromStr for Quality {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let input = s.to_ascii_lowercase();
        let ok = match input.as_str() {
            "best" | "highest" => Quality::Best,
            "worst" | "lowest " => Quality::Worst,
            _ => Quality::Custom(input), // try parsing this maybe
        };
        Ok(ok)
    }
}

#[derive(Serialize)]
pub struct Item {
    pub quality: String,
    pub resolution: String,
    pub bitrate: String,
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
            self.bitrate.parse::<f64>().unwrap() / 1024.0
        )
    }
}
