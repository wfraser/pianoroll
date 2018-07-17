use ghakuf;
use midi::*;
use note::MidiNote;

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