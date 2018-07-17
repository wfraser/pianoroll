use note::MidiNote;

#[derive(Debug)]
pub struct NoteEvent {
    pub timestamp: u64,
    pub track: usize,
    pub channel: u8,
    pub note: MidiNote,
    pub action: NoteAction,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NoteAction { On, Off }

#[derive(Debug)]
pub struct NoteWithDuration {
    pub timestamp: u64,
    pub duration: u64,
    pub note: MidiNote,
}

pub fn notes(path: &::std::path::Path) -> Result<impl Iterator<Item = NoteEvent>, String> {
    midi_impl::notes(path)
}

pub fn note_durations(
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
    let mut in_flight = BTreeMap::<MidiNote, InFlightInfo>::new();
    for event in notes {
        let offset = match filter(&event) {
            Some(offset) => offset,
            None => continue,
        };

        let note = match event.note.checked_offset(offset) {
            Some(note) if note.pianoroll_channel().is_some() => note,
            Some(_) | None => {
                println!("ERROR: at {}, offsetting note {:?} on track {} channel {} by {} puts it
                        outside of piano roll range",
                        event.timestamp, event.note, event.track, event.channel, offset);
                continue;
            }
        };

        match (event.action, in_flight.entry(note)) {
            (NoteAction::On, Entry::Vacant(entry)) => {
                entry.insert(InFlightInfo {
                    midi_track: event.track,
                    midi_channel: event.channel,
                    timestamp: event.timestamp,
                });
            }
            (NoteAction::On, Entry::Occupied(entry)) => {
                let e = entry.get();
                println!("ERROR: at {}, note {:?} on track {} channel {} already pressed at {}",
                    event.timestamp, note, e.midi_track, e.midi_channel, e.timestamp);
            }
            (NoteAction::Off, Entry::Vacant(_)) => {
                println!("ERROR: at {}, note {:?} on track {} channel {} is not pressed yet",
                    event.timestamp, note, event.track, event.channel);
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

#[cfg(feature = "nom-midi")]
mod midi_impl {
    use super::*;
    use nom_midi;

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
}

#[cfg(feature = "ghakuf")]
mod midi_impl {
    use super::*;
    use ghakuf;

    pub fn notes(path: &::std::path::Path) -> Result<impl Iterator<Item = NoteEvent>, String> {
        let mut handler = Handler::new();

        ghakuf::reader::Reader::new(&mut handler, path)
            .map_err(|e| format!("failed to read MIDI file {:?}: {}", path, e))?
            .read()
            .map_err(|e| format!("failed to parse MIDI file {:?}: {}", path, e))?;

        Ok(handler.events.into_iter())
    }

    struct Handler {
        timestamp: u64,
        track: usize,
        events: Vec<NoteEvent>,
        headers_finished: bool,
    }

    impl Handler {
        pub fn new() -> Self {
            Self {
                timestamp: 0,
                track: 0,
                events: vec![],
                headers_finished: false,
            }
        }
    }

    impl ghakuf::reader::Handler for Handler {
        fn header(&mut self, format: u16, track: u16, time_base: u16) {
            print!("MIDI file format: ");
            match format {
                0 => println!("single track"),
                1 => println!("multiple track ({})", track),
                2 => println!("multiple song ({})", track),
                _ => println!("unknown!"),
            }
            if time_base > 0 {
                println!("{} MIDI ticks per metronome beat", time_base);
            } else {
                println!("WARNING: unsupported timecode-based MIDI file");
            }
        }

        fn meta_event(
            &mut self,
            delta_time: u32,
            event: &ghakuf::messages::MetaEvent,
            data: &Vec<u8>,
        ) {
            use ghakuf::messages::MetaEvent;
            self.timestamp += u64::from(delta_time);
            match event {
                MetaEvent::SequenceOrTrackName => {
                    println!("Track {} Name: {:?}", self.track, String::from_utf8_lossy(data));
                }
                MetaEvent::InstrumentName => {
                    println!("Track {} Instrument: {:?}",
                        self.track, String::from_utf8_lossy(data));
                }
                MetaEvent::CopyrightNotice => {
                    println!("Copyright: {:?}", String::from_utf8_lossy(data));
                }
                MetaEvent::Marker => {
                    println!("Track {} at {}: {:?}",
                        self.track, self.timestamp, String::from_utf8_lossy(data));
                }
                MetaEvent::EndOfTrack => (),
                _ => {
                    println!("Track {} at {}: {:?}: {:?}",
                        self.track, self.timestamp, event, data);
                }
            }
        }

        fn midi_event(
            &mut self,
            delta_time: u32,
            event: &ghakuf::messages::MidiEvent,
        ) {
            use ghakuf::messages::MidiEvent;
            self.timestamp += u64::from(delta_time);

            match event {
                MidiEvent::NoteOn { ch, note, velocity } => {
                    let action = if *velocity == 0 {
                        NoteAction::Off
                    } else {
                        NoteAction::On
                    };

                    let note = MidiNote::try_from(*note).unwrap();

                    self.events.push(NoteEvent {
                        timestamp: self.timestamp,
                        track: self.track,
                        channel: *ch,
                        note,
                        action,
                    });
                }
                MidiEvent::NoteOff { ch, note, .. } => {
                    let note = MidiNote::try_from(*note).unwrap();

                    self.events.push(NoteEvent {
                        timestamp: self.timestamp,
                        track: self.track,
                        channel: *ch,
                        note,
                        action: NoteAction::Off,
                    });
                }
                MidiEvent::ControlChange { ch, control, data } => {
                    let off_on = |data: &u8| if *data < 64 { "off" } else { "on" };
                    let info = match control {
                        0 => Some(format!("select bank {}", data)),
                        64 => Some(format!("sustain {}", off_on(data))),
                        65 => Some(format!("portamento {}", off_on(data))),
                        66 => Some(format!("sostenuto {}", off_on(data))),
                        67 => Some(format!("soft pedal {}", off_on(data))),
                        68 => Some(format!("legato {}", off_on(data))),
                        _ => None,
                    };
                    if let Some(info) = info {
                        println!("track {}, channel {}, time {}: {}",
                            self.track, ch, self.timestamp, info);
                    }
                }
                MidiEvent::ProgramChange { ch, program } => {
                    if *program < 128 {
                        println!("track {}, channel {}, time {}: {}",
                            self.track, ch, self.timestamp,
                            ::program::MIDI_PROGRAM[*program as usize]);
                    } else {
                        println!("track {}, channel {}, time {}: set program {}",
                            self.track, ch, self.timestamp, program);
                    }
                }
                MidiEvent::PitchBendChange { .. }
                    | MidiEvent::PolyphonicKeyPressure { .. } => (),
                _ => {
                    println!("track {}, time {}, {:?}", self.track, self.timestamp, event);
                }
            }
        }

        fn sys_ex_event(
            &mut self,
            delta_time: u32,
            _event: &ghakuf::messages::SysExEvent,
            _data: &Vec<u8>,
        ) {
            self.timestamp += u64::from(delta_time);
        }

        fn track_change(&mut self) {
            if self.headers_finished {
                self.track += 1;
                self.timestamp = 0;
            } else {
                self.headers_finished = true;
            }
        }
    }
}
