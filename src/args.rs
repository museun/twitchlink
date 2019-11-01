use crate::client::Quality;
use gumdrop::Options;

#[derive(Options, Debug, Clone)]
pub struct Args {
    #[options(help = "display this message")]
    pub help: bool,

    #[options(help = "dumps the stream information as json")]
    pub json: bool,

    #[options(help = "a player to use. defaults to mpv")]
    pub player: Option<String>,

    #[options(help = "desired quality of the stream")]
    pub quality: Option<Quality>,

    #[options(help = "list stream quality information")]
    pub list: bool,

    #[options(required, free, help = "the stream to fetch")]
    pub stream: String,
}
