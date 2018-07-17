use nom_midi;
use midi::*;
use note::MidiNote;

fn parse_midi(bytes: &[u8]) -> nom_midi::Midi {
    let midi_result = nom_midi::parser::parse_midi(&bytes);
    let midi = match midi_result {
        Ok((_remaining, midi)) => midi,
        Err(e) => {
            if e.is_incomplete() {
                panic!("failed to parse MIDI file: incomplete file");
            } else {
                // Unfortunately, nom_midi doesn't have any custom error type and just returns
                // the standard nom errors, which are the most unhelpful errors ever.
                // Even getting something like the position in the file it failed at requires
                // implementing a custom input type... oh but nom_midi requires that its input
                // be `&[u8]`. >:(
                // Sorry, but this error message is just going to be terrible.
                panic!("failed to parse MIDI file: {}", e);
            }
        }
    };

    print!("MIDI file format: ");
    match midi.header.format {
        nom_midi::MidiFormat::SingleTrack => println!("single track"),
        nom_midi::MidiFormat::MultipleTrack(n) => println!("multiple track ({})", n),
        nom_midi::MidiFormat::MultipleSong(n) => println!("multiple song ({})", n),
    }
    match midi.header.division {
        nom_midi::Division::Metrical(n) => println!("{} MIDI ticks per metronome beat", n),
        nom_midi::Division::Timecode { .. } =>
            println!("WARNING: unsupported timecode-based MIDI file"),
    }

    midi
}

pub fn notes(path: &::std::path::Path) -> Result<impl Iterator<Item = NoteEvent>, String> {
    use ::std::io::Read;
    use ::std::fs::File;
    use nom_midi::{Event, EventType, MetaEvent, MidiEvent, MidiEventType, Track};

    let mut data = vec![];
    File::open(path)
        .map_err(|e| format!("Failed to open file {:?}: {}", path, e))?
        .read_to_end(&mut data)
        .map_err(|e| format!("Failed to read file {:?}: {}", path, e))?;

    let midi = parse_midi(&data);

    Ok(midi.tracks
        .into_iter()
        .enumerate()
        .flat_map(|(track, Track { events })| {
            let mut timestamp = 0u64;
            events.into_iter()
                .filter_map(move |Event { delta_time, event }| {
                    timestamp += u64::from(delta_time);
                    match event {
                        EventType::Midi(MidiEvent { channel, event }) => {
                            let (note, action) = match event {
                                MidiEventType::NoteOn(note, velocity) =>
                                    if velocity == 0 {
                                        // ON with zero velocity appears to be a proxy for OFF.
                                        // Some songs have no OFF events and just use this form.
                                        (note, NoteAction::Off)
                                    } else {
                                        (note, NoteAction::On)
                                    },
                                MidiEventType::NoteOff(note, _velocity) =>
                                    (note, NoteAction::Off),
                                _ => {
                                    // Ignore other MIDI events (controller parameter changes,
                                    // etc.)
                                    return None;
                                }
                            };

                            // change the types; this should never fail.
                            let note = MidiNote::try_from(note.into()).unwrap();

                            Some(NoteEvent {
                                timestamp,
                                track,
                                channel,
                                note,
                                action,
                            })
                        }
                        EventType::Meta(meta) => {
                            // These events aren't part of the note sequence but some are
                            // interesting for other reasons.
                            match meta {
                                MetaEvent::SequenceOrTrackName(name) => {
                                    println!("Track {} Name: {}", track, name);
                                }
                                MetaEvent::InstrumentName(name) => {
                                    println!("Track {} Instrument: {}", track, name);
                                }
                                MetaEvent::Copyright(c) => {
                                    println!("Copyright: {}", c);
                                }
                                MetaEvent::EndOfTrack => (),
                                _ => {
                                    println!("at {}: {:?}", timestamp, meta);
                                }
                            }
                            None
                        }
                        _ => None
                    }
                })
        }))
}