extern crate pdf_canvas;
extern crate ghakuf;

mod config;
use config::{Configuration, parse_configuration};

mod midi;
use midi::{note_durations, Midi, NoteAction, NoteWithDuration};

mod midi_impl_ghakuf;
mod midi_impl { pub use midi_impl_ghakuf::*; }

mod note;
mod program;

use std::collections::btree_map::*;

fn usage() {
    eprintln!("usage: {} <input.mid> [track,channel[+/-offset]...] [/timediv] [-o output.pdf]",
        std::env::args().next().unwrap());
}

fn render(notes: &[NoteWithDuration], cfg: &Configuration) {
    println!("Writing output to {:?}", cfg.output);
    let f = std::fs::File::create(&cfg.output)
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
                    note.note.pianoroll_channel().expect("note out of range"), // shouldn't happen
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
    let cfg = parse_configuration(std::env::args_os()).unwrap_or_else(|e| {
        eprintln!("{}", e);
        usage();
        std::process::exit(1);
    });

    let mut midi = Midi::new();
    midi.read(&cfg.input).unwrap();

    let time_base = midi.time_base().expect("no time base set in MIDI file?!");
    let tempo = midi.tempo().expect("no tempo set in MIDI file");

    let mut stats = std::collections::BTreeMap::<(usize, u8), u64>::new();
    let mut durations = note_durations(midi.notes(), time_base, |event| {
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

    let channels_by_track: BTreeMap<usize, Vec<&midi::ChannelInfo>>
        = midi.channels()
            .fold(BTreeMap::new(), |mut map, item| {
                match map.entry(item.midi_track) {
                    Entry::Occupied(mut entry) => { entry.get_mut().push(item); }
                    Entry::Vacant(entry) => { entry.insert(vec![item]); }
                }
                map
            });

    // Print info on the tracks and channels.
    for track in midi.tracks() {
        print!("track {}:", track.midi_track);
        if let Some(ref name) = track.name {
            print!(" title: \"{}\"", name);
        }
        if let Some(ref instrument) = track.instrument {
            print!(" instrument name: \"{}\", ", instrument);
        }
        println!();
        let channels_iter = channels_by_track
            .get(&track.midi_track)
            .map(|x| x.iter())
            .unwrap_or_else(|| [].iter());
        for channel in channels_iter {
            println!("track {}, channel {}:", channel.midi_track, channel.midi_channel);
            if channel.midi_channel == 9 {
                println!("\tPercussion");
            } else if (channel.bank == 0 || channel.bank == 121) && channel.program < 128 {
                println!("\tMIDI instrument \"{}\"",
                    program::MIDI_PROGRAM[channel.program as usize]);
            } else {
                println!("\tunknown MIDI instrument: bank {}, program {}",
                    channel.bank, channel.program);
            }
            if let Some(count) = stats.get(&(channel.midi_track, channel.midi_channel)) {
                println!("\t{} notes", count);
            } else {
                println!("\tno notes");
            }
        }
    }

    if durations.is_empty() {
        println!("no notes selected!");
    } else {
        let mut output_filename = cfg.output.file_stem().unwrap().to_owned();
        output_filename.push(std::ffi::OsStr::new("_pianoroll"));

        let midi_output = cfg.output
            .with_file_name(output_filename)
            .with_extension("mid");

        midi::Midi::write(&midi_output, &durations, time_base, tempo).unwrap();

        render(&durations, &cfg);
    }
}
