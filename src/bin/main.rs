use gumdrop::Options as _;

use twitchlink::*;

trait Abort<T, E = ()> {
    fn unwrap_or_abort<F: FnOnce(E) -> S, S: std::fmt::Display>(self, f: F) -> T;
}

impl<T, E: std::fmt::Display> Abort<T, E> for Result<T, E> {
    fn unwrap_or_abort<F: FnOnce(E) -> S, S: std::fmt::Display>(self, f: F) -> T {
        self.unwrap_or_else(|err| fatal(f(err)))
    }
}
impl<T> Abort<T, ()> for Option<T> {
    fn unwrap_or_abort<F: FnOnce(()) -> S, S: std::fmt::Display>(self, f: F) -> T {
        self.unwrap_or_else(|| fatal(f(())))
    }
}

fn error(msg: impl std::fmt::Display) {
    eprintln!("error: {}", msg);
}

fn fatal(msg: impl std::fmt::Display) -> ! {
    eprintln!("fatal error: {}", msg);
    std::process::exit(1)
}

fn get_channel_name(input: &str) -> &str {
    // TODO be smarter about this
    if input.contains('/') {
        input.split('/').last().unwrap()
    } else {
        input
    }
}

#[derive(Copy, Clone, PartialEq)]
enum Output {
    PrintAll,
    PrintAllJson,
    PrintOne,
    PrintOneJson,
    PrintStreamsJson,
    OpenPlayer,
}

impl Output {
    fn from_args(json: bool, list: bool, is_singular: bool) -> Self {
        match (json, list, is_singular) {
            (false, true, false) => Self::PrintAll,
            (false, true, true) => Self::PrintOne,
            (true, true, true) => Self::PrintOneJson,
            (true, true, false) => Self::PrintAllJson,
            (true, false, _) => Self::PrintStreamsJson,
            _ => Self::OpenPlayer,
        }
    }
}

fn main() {
    let player = std::env::var("TWITCHLINK_PLAYER").ok().unwrap_or_else(|| {
        if cfg!(not(windows)) {
            "/usr/bin/mpv".to_string()
        } else {
            "mpv".to_string()
        }
    });

    // TODO show the version
    let args = Args::parse_args_default_or_exit();

    let player = args.player.unwrap_or_else(|| player);
    let channel = get_channel_name(&args.stream);
    let is_singular = args.quality.is_some();
    let quality = args.quality.unwrap_or_else(|| Quality::Best);

    let id = std::env::var("TWITCH_CLIENT_ID").unwrap_or_abort(|_| {
        "Error: The environment variable 'TWITCH_CLIENT_ID' must be set to your Twitch client ID"
    });

    let streams = client::get(&id, &channel).unwrap_or_abort(|err| err);

    let stream = match quality {
        Quality::Best => streams
            .first()
            .unwrap_or_abort(|_| format!("stream `{}` is offline", channel)),

        Quality::Lowest => streams
            .last()
            .unwrap_or_abort(|_| format!("stream `{}` is offline", channel)),

        Quality::Custom(mut s) => {
            if !s.ends_with('p') {
                s.push('p');
            }
            streams
                .iter()
                .find(|stream| stream.ty == *s)
                .unwrap_or_abort(|_| {
                    format!("quality `{}` is not available for stream `{}` ", s, channel)
                })
        }
    };

    match Output::from_args(args.json, args.list, is_singular) {
        Output::PrintAll => {
            for stream in streams.into_iter().map(Item::from) {
                println!("{}", stream)
            }
        }
        Output::PrintAllJson => {
            let items = streams.into_iter().map(Item::from).collect::<Vec<_>>();
            println!("{}", serde_json::to_string(&items).unwrap());
        }
        Output::PrintOne => {
            println!("{}", Item::from(stream.clone()));
        }
        Output::PrintOneJson => {
            println!(
                "{}",
                serde_json::to_string(&Item::from(stream.clone())).unwrap()
            );
        }
        Output::OpenPlayer => {
            if std::fs::metadata(&player).is_err() {
                fatal(format!("invalid path: {}. set `TWITCHLINK_PLAYER` or provide a path to a valid executable", player));
            }
            if let Err(err) = std::process::Command::new(&player)
                .arg(&stream.link)
                .spawn()
            {
                fatal(format!(
                    "cannot start stream `{}`. make sure `{}` is a valid player\nerror: {}",
                    channel, player, err
                ))
            }
        }
        Output::PrintStreamsJson => {
            let s = if !is_singular {
                serde_json::to_string(&streams)
            } else {
                serde_json::to_string(&stream)
            };
            println!("{}", s.unwrap());
        }
    }
}
