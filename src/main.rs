extern crate pdf_canvas;

#[cfg(feature = "nom-midi")]
extern crate nom_midi;
#[cfg(feature = "ghakuf")]
extern crate ghakuf;

use std::fs::File;

mod config;
use config::{Configuration, parse_configuration};

mod midi;
use midi::{notes, note_durations, NoteAction, NoteWithDuration};

mod note;
mod program;

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

    let notes = notes(&cfg.input).unwrap();

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
        println!("no notes selected!");
    } else {
        render(&durations, &cfg);
    }
}
