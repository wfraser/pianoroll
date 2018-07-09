use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Configuration {
    pub input: PathBuf,
    pub output: PathBuf,
    pub selectors: Vec<ChannelSelector>,
    pub time_divisor: f32,
}

#[derive(Debug)]
pub struct ChannelSelector {
    pub midi_track: usize,
    pub midi_channel: u8,
    pub offset: i8,
}

pub fn parse_configuration(args: impl Iterator<Item = OsString>) -> Result<Configuration, String> {
    let mut input = None;
    let mut output = None;
    let mut selectors = vec![];
    let mut time_divisor = None;

    let mut skip = 0;
    let mut args = args.skip(1).peekable();
    while let Some(arg) = args.next() {
        if skip > 0 {
            skip -= 1;
            continue;
        }
        if arg == OsStr::new("-o") {
            let next_arg = args.peek()
                .ok_or_else(|| "-o must be followed by another argument".to_owned())?;
            output = Some(PathBuf::from(next_arg));
            skip = 1;
        } else if input.is_none() {
            input = Some(PathBuf::from(arg));
        } else {
            let arg = arg.to_str().ok_or_else(|| format!("non-utf8 argument {:?}", arg))?;
            // channel selector or timediv
            if arg.starts_with('/') {
                time_divisor = Some(arg[1..].parse()
                    .map_err(|e| format!("time divisor parse error: {}", e))?);
            } else {
                let selector = parse_track_selector(arg)
                    .map_err(|e| format!("malformed track selector \"{}\": {}", arg, e))?;
                selectors.push(selector);
            }
        }
    }

    let input = input.ok_or_else(|| "missing input argument".to_owned())?;
    let output = output.unwrap_or_else(|| input.with_extension("pdf"));
    let time_divisor = time_divisor.unwrap_or(1.);
    Ok(Configuration {
        input,
        output,
        selectors,
        time_divisor,
    })
}

fn parse_track_selector(arg: &str) -> Result<ChannelSelector, String> {
    let mut track_parts = arg.splitn(2, ',');
    let track: usize = track_parts.next()
        .ok_or_else(|| "expected a ','".to_owned())?
        .parse()
        .map_err(|e| format!("bad track number: {}", e))?;
    let channel_rest = track_parts.next()
        .ok_or_else(|| "expected a ','".to_owned())?;
    let (channel, offset): (u8, i8) = match channel_rest.find(|c| c == '+' || c == '-') {
        Some(plusminus_pos) => {
            let (channel_str, offset_str) = channel_rest.split_at(plusminus_pos);
            let channel: u8 = channel_str.parse()
                .map_err(|e| format!("bad channel number: {}", e))?;
            let offset: i8 = offset_str.parse()
                .map_err(|e| format!("bad offset number: {}", e))?;
            (channel, offset)
        }
        None => {
            let channel: u8 = channel_rest.parse()
                .map_err(|e| format!("bad channel number: {}", e))?;
            (channel, 0)
        }
    };
    Ok(ChannelSelector {
        midi_track: track,
        midi_channel: channel,
        offset,
    })
}
