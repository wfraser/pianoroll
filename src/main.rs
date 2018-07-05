extern crate pdf_canvas;
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

#[derive(Debug)]
struct NoteDuration {
    timestamp: u64,
    duration: u64,
    track: usize,
    channel: u8,
    note: Note,
}

#[derive(Debug, Eq, PartialEq)]
enum NoteAction { On, Off }

fn notes(midi: nom_midi::Midi) -> impl Iterator<Item = NoteEvent> {
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
                                }
                                _ => {
                                    println!("at {}: {:?}", timestamp, meta);
                                }
                            }
                            None
                        }
                        _ => None
                    }
                })
        })
}

fn note_durations(notes: impl Iterator<Item = NoteEvent>) -> Vec<NoteDuration> {
    use std::collections::btree_map::*;

    let mut finished_notes: Vec<NoteDuration> = vec![];
    let mut in_flight = BTreeMap::<(usize, u8), BTreeMap<i8, u64>>::new();
    for event in notes {
        let key = (event.track, event.channel);
        let note: i8 = event.note.into();
        let map = in_flight.entry(key).or_insert_with(|| BTreeMap::new());
        let entry = map.entry(note);
        match (event.action, entry) {
            (NoteAction::On, Entry::Vacant(entry)) => {
                entry.insert(event.timestamp);
            }
            (NoteAction::On, Entry::Occupied(entry)) => {
                panic!("note {:?} already pressed at {}", event.note, entry.get());
            }
            (NoteAction::Off, Entry::Vacant(_)) => {
                panic!("note {:?} is not pressed yet at {}", event.note, event.timestamp);
            }
            (NoteAction::Off, Entry::Occupied(entry)) => {
                let start_timestamp = entry.remove();
                let duration = event.timestamp - start_timestamp;
                finished_notes.push(NoteDuration {
                    timestamp: start_timestamp,
                    duration,
                    track: event.track,
                    channel: event.channel,
                    note: event.note,
                });
            }
        }
    }

    finished_notes
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

    let mut stats = std::collections::BTreeMap::<(usize, u8), u64>::new();
    let mut notes = notes(midi)
        .inspect(|event| *stats.entry((event.track, event.channel)).or_insert(0) += 1)
        .filter_map(|mut event| {
            let note_val: i8 = event.note.into();
            if note_val < Note::A0.into() {
                panic!("event {:?} is out of 88-key piano range", event);
            } else if note_val > Note::C7.into() {
                panic!("event {:?} is out of 88-key piano range", event);
            }
            /*
            // for experiment purposes, try selecting two tracks and shifting each one octave away
            let offset = match (event.track, event.channel) {
                (1,0) => 12,
                (2,1) => -12,
                _ => return None
            };
            event.note = Note::from(note_val + offset);
            */
            Some(event)
        })
        .collect::<Vec<NoteEvent>>();
    notes.sort_by_key(|event| event.timestamp);
    //println!("{:#?}", notes);

    for entry in stats {
        println!("track {}, channel {}: {}", entry.0 .0, entry.0 .1, entry.1);
    }

    let mut durations = note_durations(notes.into_iter());
    durations.sort_by_key(|event| event.timestamp);
    println!("{:#?}", durations);

    let mut pdf = pdf_canvas::Pdf::create("out.pdf")
        .expect("failed to create PDF");

    const POINTS_PER_INCH: f32 = 72.;
    const PAGE_WIDTH: f32 = POINTS_PER_INCH * 11.25;
    const CHANNEL_WIDTH: f32 = POINTS_PER_INCH / 9.;
    const PAGE_MARGIN: f32 = (PAGE_WIDTH - CHANNEL_WIDTH * 98.) / 2.;
    const HOLE_WIDTH: f32 = CHANNEL_WIDTH / 2.;
    const HOLE_MARGIN: f32 = CHANNEL_WIDTH / 4.;

    fn note_rectangle(canvas: &mut pdf_canvas::Canvas, channel: u8, start: f32, height: f32) -> Result<(), std::io::Error> {
        canvas.rectangle(
            channel as f32 * CHANNEL_WIDTH + HOLE_MARGIN + PAGE_MARGIN,
            start,
            HOLE_WIDTH,
            height,
        )
    }

    pdf.render_page(PAGE_WIDTH, 100. * POINTS_PER_INCH,
        |canvas| {
            canvas.set_fill_color(pdf_canvas::graphicsstate::Color::gray(0))?;
            for i in 0 .. 97 {
                note_rectangle(canvas, i, i as f32 * 10., HOLE_WIDTH)?;
            }
            canvas.fill()?;
            Ok(())
        })
        .expect("failed to render page");

    pdf.finish()
        .expect("failed to finish PDF");
}
