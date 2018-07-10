extern crate pdf_canvas;
extern crate nom_midi;
use nom_midi::note::Note as MidiNote;

use std::fs::File;

mod config;
use config::{Configuration, parse_configuration};

/// This represents the raw stream of events from the MIDI file.
#[derive(Debug)]
struct NoteEvent {
    timestamp: u64,
    track: usize,
    channel: u8,
    note: MidiNote,
    action: NoteAction,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum NoteAction { On, Off }

#[derive(Debug)]
struct NoteWithDuration {
    timestamp: u64,
    duration: u64,
    note: Note,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Note {
    inner: MidiNote,
}

impl Note {
    pub fn try_from(inner: nom_midi::note::Note) -> Option<Self> {
        let raw: u8 = inner.into();
        if raw < MidiNote::C1.into()
            || raw > MidiNote::G7.into()
        {
            None
        } else {
            Some(Note { inner })
        }
    }

    pub fn pianoroll_channel(self) -> u8 {
        // [0-5] = bass expressions
        // [6-7] = soft pedal
        // 8 = C1
        // ...
        // 87 = G7
        // 88 = rewind
        // 89 = blank
        // [90-91] = sustain pedal
        // [92-97] = treble expressions
        let raw: u8 = self.inner.into();
        let base: u8 = MidiNote::C1.into();
        raw - base
    }
}

impl Ord for Note {
    fn cmp(&self, other: &Note) -> std::cmp::Ordering {
        let raw: u8 = self.inner.into();
        let other_raw: u8 = other.inner.into();
        raw.cmp(&other_raw)
    }
}

impl PartialOrd for Note {
    fn partial_cmp(&self, other: &Note) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Note {}

fn parse_midi(bytes: &[u8]) -> nom_midi::Midi {
    let midi_result = nom_midi::parser::parse_midi(&bytes);
    let midi = if midi_result.is_incomplete() {
        panic!("failed to parse MIDI file: incomplete file");
    } else if midi_result.is_err() {
        // Unfortunately, nom_midi doesn't have any custom error type and just returns the standard
        // nom errors, which are the most unhelpful errors ever.
        // Even getting something like the position in the file it failed at requires implementing a
        // custom input type... oh but nom_midi requires that its input be `&[u8]`. >:(
        // Sorry, but this error message is just going to be terrible.
        panic!("failed to parse MIDI file: {:?}", midi_result.unwrap_err());
    } else {
        midi_result.unwrap().1
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

fn notes(data: &[u8]) -> impl Iterator<Item = NoteEvent> {
    use nom_midi::{Event, EventType, MetaEvent, MidiEvent, MidiEventType, Track};

    let midi = parse_midi(data);

    midi.tracks
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
        })
}

fn note_durations(
    notes: impl Iterator<Item = NoteEvent>,
    mut filter: impl FnMut(&NoteEvent) -> Option<i8>,
) -> Vec<NoteWithDuration> {
    use std::collections::btree_map::*;

    #[derive(Debug)]
    struct InFlightInfo {
        midi_track: usize,
        midi_channel: u8,
        timestamp: u64,
    }

    let mut finished_notes: Vec<NoteWithDuration> = vec![];
    let mut in_flight = BTreeMap::<Note, InFlightInfo>::new();
    for event in notes {
        let offset = match filter(&event) {
            Some(offset) => offset,
            None => continue,
        };

        let raw: i8 = event.note.into();
        let after_offset = MidiNote::from(raw + offset);

        let note = Note::try_from(after_offset)
            .unwrap_or_else(||
                panic!("note {:?} from {:?} is out of piano roll range", after_offset, event));

        match (event.action, in_flight.entry(note)) {
            (NoteAction::On, Entry::Vacant(entry)) => {
                entry.insert(InFlightInfo {
                    midi_track: event.track,
                    midi_channel: event.channel,
                    timestamp: event.timestamp,
                });
            }
            (NoteAction::On, Entry::Occupied(entry)) => {
                panic!("note {:?} already pressed by {:?}", note.inner, entry.get());
            }
            (NoteAction::Off, Entry::Vacant(_)) => {
                panic!("note {:?} is not pressed yet at {}", note.inner, event.timestamp);
            }
            (NoteAction::Off, Entry::Occupied(entry)) => {
                let start_timestamp = entry.remove().timestamp;
                let duration = event.timestamp - start_timestamp;
                finished_notes.push(NoteWithDuration {
                    timestamp: start_timestamp,
                    duration,
                    note,
                });
            }
        }
    }

    finished_notes
}

fn usage() {
    eprintln!("usage: {} <input.mid> [track,channel[+/-offset]...] [/timediv] [-o output.pdf]",
        std::env::args().nth(0).unwrap());
}

fn render(notes: &[NoteWithDuration], cfg: &Configuration) {
    println!("Writing output to {:?}", cfg.output);
    let f = File::create(&cfg.output)
        .unwrap_or_else(|e| panic!("failed to create PDF file {:?}: {}", &cfg.output, e));
    let mut pdf = pdf_canvas::Pdf::new(f)
        .expect("failed to create PDF");

    const POINTS_PER_INCH: f32 = 72.;
    const PAGE_WIDTH: f32 = POINTS_PER_INCH * 11.25;
    const CHANNEL_WIDTH: f32 = POINTS_PER_INCH / 9.;
    const PAGE_MARGIN: f32 = (PAGE_WIDTH - CHANNEL_WIDTH * 98.) / 2.;
    const HOLE_WIDTH: f32 = CHANNEL_WIDTH / 2.;
    const HOLE_MARGIN: f32 = CHANNEL_WIDTH / 4.;

    fn note_rectangle(canvas: &mut pdf_canvas::Canvas, channel: u8, start: f32, height: f32)
        -> Result<(), std::io::Error>
    {
        canvas.rectangle(
            f32::from(channel) * CHANNEL_WIDTH + HOLE_MARGIN + PAGE_MARGIN,
            start,
            HOLE_WIDTH,
            height,
        )
    }

    let end_timestamp = notes.iter()
        .map(|elem| elem.timestamp + elem.duration)
        .max()
        .unwrap();

    let page_height = end_timestamp as f32 / cfg.time_divisor;
    println!("piano roll length: {} inches", page_height / POINTS_PER_INCH);
    if page_height / POINTS_PER_INCH > 200. {
        println!("WARNING: exceeding PDF page height limit of 200 inches");
    }

    pdf.render_page(PAGE_WIDTH, page_height,
        |canvas| {
            canvas.set_fill_color(pdf_canvas::graphicsstate::Color::gray(0))?;
            for note in notes {
                note_rectangle(
                    canvas,
                    note.note.pianoroll_channel(),
                    note.timestamp as f32 / cfg.time_divisor,
                    note.duration as f32 / cfg.time_divisor)?;
                canvas.fill()?;
            }

            Ok(())
        })
        .expect("failed to render page");

    pdf.finish()
        .expect("failed to finish PDF");
}

fn main() {
    use std::io::Read;

    let cfg = parse_configuration(std::env::args_os()).unwrap_or_else(|e| {
        eprintln!("{}", e);
        usage();
        std::process::exit(1);
    });

    let mut bytes = vec![];
    File::open(&cfg.input)
        .expect("failed to open file")
        .read_to_end(&mut bytes)
        .expect("failed to read file");

    let notes = notes(&bytes);

    let mut stats = std::collections::BTreeMap::<(usize, u8), u64>::new();
    let mut durations = note_durations(notes.into_iter(), |event| {
        // Make stats on how many notes are in each track/channel.
        if event.action == NoteAction::On {
            *stats.entry((event.track, event.channel)).or_insert(0) += 1;
        }

        for selector in &cfg.selectors {
            if event.track == selector.midi_track
                && event.channel == selector.midi_channel
            {
                return Some(selector.offset);
            }
        }

        None
    });
    durations.sort_by_key(|event| event.timestamp);

    for entry in stats {
        println!("track {}, channel {}: {} notes", entry.0 .0, entry.0 .1, entry.1);
    }

    if durations.is_empty() {
        panic!("no notes selected!");
    }

    render(&durations, &cfg);
}
