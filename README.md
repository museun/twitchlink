## twitchlink 
### Version: 0.1.0
a simple utility to open a twitch stream in a local player

the environment variable `TWITCH_CLIENT_ID` must be set

if the environment variable `STREAMLINK_PLAYER` is set, it'll provide the default for `-p flag`. if its not set and `-p` is not used, then `mpv` is attempted.

### usage
```
twitchlink [OPTIONS]

Positional arguments:
  stream                 the stream to fetch

Optional arguments:
  -h, --help             display this message
  -j, --json             dumps the stream information as json        
  -p, --player PLAYER    a player to use.
  -q, --quality QUALITY  desired quality of the stream
  -l, --list             list stream quality information
```
