use ghakuf::{self, messages::{Message, MetaEvent, MidiEvent}};
use midi::*;
use note::MidiNote;
use std::collections::btree_map::*;

#[derive(Debug)]
pub struct MidiImpl {
    track_info: Vec<TrackInfo>,
    channel_info: Vec<ChannelInfo>,
    note_events: Vec<NoteEvent>,
    time_base: Option<u16>,
    tempo: Option<u32>,
}

impl MidiImpl {
    pub fn new() -> Self {
        Self {
            track_info: vec![],
            channel_info: vec![],
            note_events: vec![],
            time_base: None,
            tempo: None,
        }
    }

    pub fn read(&mut self, path: &::std::path::Path) -> Result<(), String> {
        let mut song_info_handler = SongInfoHandler::new();
        let mut notes_handler = NotesHandler::new();
        let mut channel_handler = ChannelInfoHandler::new();

        {
            let mut g = ghakuf::reader::Reader::new(&mut song_info_handler, path)
                .map_err(|e| format!("failed to read MIDI file {:?}: {}", path, e))?;

            g.push_handler(&mut notes_handler);
            g.push_handler(&mut channel_handler);

            g.read()
                .map_err(|e| format!("failed to parse MIDI file {:?}: {}", path, e))?;
        }

        self.note_events = notes_handler.events;
        self.channel_info = channel_handler.channel_info().collect();
        self.track_info = channel_handler.track_info().collect();
        self.time_base = song_info_handler.time_base;
        self.tempo = song_info_handler.tempo;

        Ok(())
    }

    pub fn tracks(&self) -> impl Iterator<Item = &TrackInfo> {
        self.track_info.iter()
    }

    pub fn channels(&self) -> impl Iterator<Item = &ChannelInfo> {
        self.channel_info.iter()
    }

    pub fn notes(&self) -> impl Iterator<Item = &NoteEvent> {
        self.note_events.iter()
    }

    pub fn time_base(&self) -> Option<u16> {
        self.time_base
    }

    pub fn tempo(&self) -> Option<u32> {
        self.tempo
    }

    pub fn write(path: &::std::path::Path, notes: &[NoteWithDuration], time_base: u16, tempo: u32)
        -> Result<(), String>
    {
        const VELOCITY: u8 = 90; // arbitrary but seems to sound good

        let mut messages = vec![
            Message::MetaEvent {
                delta_time: 0,
                event: MetaEvent::SetTempo,
                data: [(tempo >> 16) as u8, (tempo >> 8) as u8, tempo as u8].to_vec(),
            },
            Message::MetaEvent {
                delta_time: 0,
                event: MetaEvent::EndOfTrack,
                data: Vec::new(),
            },
            Message::TrackChange,
            Message::MidiEvent {
                delta_time: 0,
                event: MidiEvent::ControlChange {
                    ch: 0,
                    control: 0,
                    data: 0,
                }
            },
            Message::MidiEvent {
                delta_time: 0,
                event: MidiEvent::ProgramChange {
                    ch: 0,
                    program: 1,
                },
            },
        ];

        let mut note_events = vec![];
        for note in notes {
            note_events.push(NoteEvent {
                timestamp: note.timestamp,
                track: 0,
                channel: 0,
                note: note.note,
                action: NoteAction::On,
            });
            note_events.push(NoteEvent {
                timestamp: note.timestamp + note.duration,
                track: 0,
                channel: 0,
                note: note.note,
                action: NoteAction::Off,
            });
        }
        note_events.sort_by_key(|event| event.timestamp);

        let mut last_timestamp = 0;
        for note in note_events {
            let event = match note.action {
                NoteAction::On => MidiEvent::NoteOn {
                    ch: note.channel,
                    note: note.note.as_u8(),
                    velocity: VELOCITY,
                },
                NoteAction::Off => MidiEvent::NoteOff {
                    ch: note.channel,
                    note: note.note.as_u8(),
                    velocity: VELOCITY,
                },
            };
            let msg = Message::MidiEvent {
                delta_time: (note.timestamp - last_timestamp) as u32,
                event,
            };
            messages.push(msg);
            last_timestamp = note.timestamp;
        }
        messages.push(
            Message::MetaEvent {
                delta_time: 0,
                event: MetaEvent::EndOfTrack,
                data: Vec::new(),
            });

        let mut writer = ghakuf::writer::Writer::new();
        writer.time_base(time_base);
        for message in &messages {
            writer.push(&message);
        }

        writer.write(path)
            .map_err(|e| format!("Error writing MIDI: {}", e))
    }
}

struct NotesHandler {
    timestamp: u64,
    track: usize,
    events: Vec<NoteEvent>,
    headers_finished: bool,
}

impl NotesHandler {
    pub fn new() -> Self {
        Self {
            timestamp: 0,
            track: 0,
            events: vec![],
            headers_finished: false,
        }
    }
}

impl ghakuf::reader::Handler for NotesHandler {
    fn meta_event(
        &mut self,
        delta_time: u32,
        _event: &MetaEvent,
        _data: &Vec<u8>,
    ) {
        self.timestamp += u64::from(delta_time);
    }

    fn midi_event(
        &mut self,
        delta_time: u32,
        event: &MidiEvent,
    ) {
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
            /*
            MidiEvent::ControlChange { ch, control, data } => {
                let off_on = |data: &u8| if *data < 64 { "off" } else { "on" };
                let info = match control {
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
            */
            MidiEvent::ControlChange { .. } => (),
            MidiEvent::ChannelPressure { .. }
                | MidiEvent::PitchBendChange { .. }
                | MidiEvent::PolyphonicKeyPressure { .. }
                | MidiEvent::ProgramChange { .. } => (),
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

struct TrackName {
    name: Option<String>,
    instrument: Option<String>,
}

struct ChannelName {
    bank: Option<u8>,
    program: Option<u8>,
}

struct ChannelInfoHandler {
    track: usize,
    headers_finished: bool,
    tracks: BTreeMap<usize, TrackName>,
    channels: BTreeMap<(usize, u8), ChannelName>,
}

impl ChannelInfoHandler {
    pub fn new() -> Self {
        Self {
            track: 0,
            headers_finished: false,
            tracks: BTreeMap::new(),
            channels: BTreeMap::new(),
        }
    }

    pub fn track_info<'a>(&'a self) -> impl Iterator<Item = TrackInfo> + 'a {
        self.tracks.iter().map(move |(track, v)| {
            TrackInfo {
                midi_track: *track,
                name: v.name.clone(),
                instrument: v.instrument.clone(),
            }
        })
    }

    pub fn channel_info<'a>(&'a self) -> impl Iterator<Item = ChannelInfo> + 'a {
        self.channels.iter().filter_map(move |((track, channel), v)| {
            let bank = match v.bank {
                Some(bank) => bank,
                None => {
                    println!("ERROR: track {} channel {} has no MIDI bank set", track, channel);
                    0 // use a default value
                }
            };
            let program = match v.program {
                Some(program) => program,
                None => {
                    println!("ERROR: track {} channel {} has no MIDI program set", track, channel);
                    0 // use a default value
                }
            };
            Some(ChannelInfo {
                midi_track: *track,
                midi_channel: *channel,
                bank,
                program,
            })
        })
    }
}

impl ghakuf::reader::Handler for ChannelInfoHandler {
    fn meta_event(
        &mut self,
        _delta_time: u32,
        event: &ghakuf::messages::MetaEvent,
        data: &Vec<u8>,
    ) {
        let track_entry = self.tracks.entry(self.track)
            .or_insert_with(||
                TrackName {
                    name: None,
                    instrument: None,
                });
        match event {
            MetaEvent::SequenceOrTrackName => {
                let name = String::from_utf8_lossy(data).into_owned();
                if track_entry.name.is_none() {
                    track_entry.name = Some(name);
                } else {
                    println!("WARNING: track {} given multiple names: {:?}",
                                self.track, name);
                }
            }
            MetaEvent::InstrumentName => {
                let name = String::from_utf8_lossy(data).into_owned();
                if track_entry.instrument.is_none() {
                    track_entry.instrument = Some(name);
                } else {
                    println!("WARNING: track {} given multiple instrument names: {:?}",
                        self.track, name);
                }
            },
            _ => (),
        }
    }

    fn midi_event(
        &mut self,
        _delta_time: u32,
        event: &MidiEvent,
    ) {
        match event {
            MidiEvent::ControlChange { ch, control, data } if *control == 0 => {
                let entry = self.channels.entry((self.track, *ch))
                    .or_insert(ChannelName { bank: None, program: None });
                if entry.bank.is_none() {
                    entry.bank = Some(*data);
                } else {
                    println!("WARNING: track {} set to another bank ({}) mid-song",
                        self.track, data);
                }
            }
            /*MidiEvent::ControlChange { control, .. } if *control == 32 => {
                // In Roland GS, CC#0 is the bank select MSB, and CC#32 is the bank select LSB
                // Should probably handle this...
                println!("{:#?}", event);
            }*/
            MidiEvent::ProgramChange { ch, program } => {
                let entry = self.channels.entry((self.track, *ch))
                    .or_insert(ChannelName { bank: None, program: None });
                if entry.program.is_none() {
                    entry.program = Some(*program);
                } else {
                    println!("WARNING: track {} set to another program ({}) mid-song",
                        self.track, program);
                }
            }
            MidiEvent::NoteOn { ch, .. } => {
                let _entry = self.channels.entry((self.track, *ch))
                    .or_insert(ChannelName { bank: None, program: None });
                // do nothing with it; just make one if there wasn't one before.
            }
            _ => (),
        }
    }

    fn track_change(&mut self) {
        if self.headers_finished {
            self.track += 1;
        } else {
            self.headers_finished = true;
        }
    }
}

struct SongInfoHandler {
    time_base: Option<u16>,
    tempo: Option<u32>,
}

impl SongInfoHandler {
    pub fn new() -> Self {
        Self {
            time_base: None,
            tempo: None,
        }
    }
}

impl ghakuf::reader::Handler for SongInfoHandler {
    fn header(&mut self, format: u16, track: u16, time_base: u16) {
        print!("MIDI file format: ");
        match format {
            0 => println!("single track"),
            1 => println!("multiple track ({})", track),
            2 => println!("multiple song ({})", track),
            _ => println!("unknown!"),
        }
        if time_base > 0 {
            self.time_base = Some(time_base);
            println!("{} MIDI ticks per metronome beat", time_base);
        } else {
            println!("WARNING: unsupported timecode-based MIDI file");
        }
    }

    fn meta_event(
        &mut self,
        _delta_time: u32,
        event: &ghakuf::messages::MetaEvent,
        data: &Vec<u8>,
    ) {
        match event {
            MetaEvent::CopyrightNotice => {
                println!("Copyright: {:?}", String::from_utf8_lossy(data));
            }
            MetaEvent::SetTempo => {
                let mut micros = 0u32; // microseconds per beat
                for byte in data {
                    micros <<= 8;
                    micros += u32::from(*byte);
                }
                if self.tempo.is_some() {
                    println!("WARNING: tempo changes are not supported; using new tempo");
                }
                self.tempo = Some(micros);
                println!("Tempo: {} beats per minute", 60_000_000 / micros);
            }
            MetaEvent::Marker => {
                println!("Marker: {:?}", String::from_utf8_lossy(data));
            }
            MetaEvent::TextEvent => {
                println!("Text: {:?}", String::from_utf8_lossy(data));
            }
            _ => ()
        }
    }
}
