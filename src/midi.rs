use midi_impl;
use note::MidiNote;

#[derive(Debug, Clone)]
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

#[derive(Debug)]
pub struct TrackInfo {
    pub midi_track: usize,
    pub name: Option<String>,
    pub instrument: Option<String>,
}

#[derive(Debug)]
pub struct ChannelInfo {
    pub midi_track: usize,
    pub midi_channel: u8,
    pub bank: u8,
    pub program: u8,
}

#[derive(Debug)]
pub struct Midi {
    midi_impl: midi_impl::MidiImpl,
}

impl Midi {
    pub fn new() -> Self {
        Self {
            midi_impl: midi_impl::MidiImpl::new(),
        }
    }

    pub fn read(&mut self, path: &::std::path::Path) -> Result<
        (impl Iterator<Item = &NoteEvent>, impl Iterator<Item = &ChannelInfo>),
        String>
    {
        self.midi_impl.read(path)?;
        Ok((self.midi_impl.notes(), self.midi_impl.channels()))
    }

    pub fn tracks(&self) -> impl Iterator<Item = &TrackInfo> {
        self.midi_impl.tracks()
    }

    pub fn channels(&self) -> impl Iterator<Item = &ChannelInfo> {
        self.midi_impl.channels()
    }

    pub fn notes(&self) -> impl Iterator<Item = &NoteEvent> {
        self.midi_impl.notes()
    }
}

pub fn note_durations<'a>(
    notes: impl Iterator<Item = &'a NoteEvent>,
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