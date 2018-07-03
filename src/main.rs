extern crate nom_midi;
use nom_midi::note::Note;

#[derive(Debug)]
struct NoteEvent {
    timestamp: u64,
    track: usize,
    channel: u8,
    note: Note,
    action: NoteAction,
}

#[derive(Debug, Eq, PartialEq)]
enum NoteAction { On, Off }

fn notes(midi: nom_midi::Midi) -> Vec<NoteEvent> {
    use nom_midi::{Event, EventType, MetaEvent, MidiEvent, MidiEventType, Track};

    midi.tracks
        .into_iter()
        .enumerate()
        .flat_map(|(track, Track { events })| {
            let mut timestamp = 0u64;
            events.into_iter()
                .filter_map(move |Event { delta_time, event }| {
                    timestamp += delta_time as u64;
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
                                    // Ignore other MIDI events (controller parameter changes, etc.)
                                    return None;
                                }
                            };

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
                                },
                                _ => ()
                            }
                            None
                        }
                        _ => None
                    }
                })
        })
        .collect()
}

fn main() {
    use std::io::Read;
    let mut bytes = vec![];
    let path = std::env::args_os().nth(1).expect("missing file argument");
    std::fs::File::open(path)
        .expect("failed to open file")
        .read_to_end(&mut bytes)
        .expect("failed to read file");

    let (_remaining, midi) = nom_midi::parser::parse_midi(&bytes).unwrap();
    println!("{:#?}", midi.header);

    let mut notes = notes(midi);
    notes.sort_by_key(|event| event.timestamp);
    println!("{:#?}", notes)
}
