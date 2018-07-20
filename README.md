# pianoroll

Pianoroll is an experimental project to make player piano rolls from MIDI
files.

Currently the software is able to take a MIDI file and generate a large
single-page PDF of a piano roll for the song. How to actually turn this into a
physical piece of paper with holes punched in it is still under investigation.

## Building

`pianoroll` is written in Rust, and can be built with the standard Rust toolchain, Cargo.
See https://rustup.rs/ for how to install and run the Rust compiler and Cargo.

## Operation

To start, run the program with your chosen `.mid` file as the only argument. `pianoroll` will
display some info about the file:

```
% cargo run take5.mid
    Finished dev [unoptimized + debuginfo] target(s) in 0.05s
     Running `target/debug/pianoroll take5.mid`
MIDI file format: single track
96 MIDI ticks per metronome beat
Copyright: "(C)1991 Roland Corporation"
Tempo: 180 beats per minute
track 0: title: "Take Five"
track 0, channel 0:
        MIDI instrument "Acoustic Grand Piano"
        1394 notes
track 0, channel 1:
        MIDI instrument "Acoustic Bass"
        509 notes
track 0, channel 2:
        MIDI instrument "Electric Guitar (jazz)"
        769 notes
track 0, channel 3:
        MIDI instrument "Alto Sax"
        328 notes
track 0, channel 4:
        MIDI instrument "Pad 2 (warm)"
        no notes
track 0, channel 9:
        Percussion
        1898 notes
track 0, channel 10:
        MIDI instrument "Acoustic Bass"
        539 notes
no notes selected!
```

This file has one track with multiple channels (as opposed to the other common format which is
multiple tracks with a single channel each).

MIDI files specify multiple instruments, but we have to mix them down to one (the piano) somehow.
Each (track, channel) pair identifies an instrument in the song, so select one or more of them to
be mixed into the final song.

This is done by specifying additional arguments to `pianoroll` after
the MIDI file path, in the form `track,channel`, optionally followed by `+notes` or `-notes` to
shift the part up or down by that many notes (remember that 12 notes make an octave, so you'll
probably want to stick to multiples of 12 when doing this). Shifting parts can be useful when two
instruments occupy roughly the same note range and you want them to stand apart from each other.

A good starting strategy is to select each track+channel separately, one at a time, and see what
they sound like. Then you can decide which ones to mix together. `pianoroll` will output a PDF as
well as a MIDI file (with the suffix `_pianoroll`) that simulates the player piano playing those
notes you selected.

## Example

For the sample file of Take Five, I find that selecting the Acoustic Grand Piano part, the Alto Sax,
and the Acoustic Bass parts sound good together, after shifting the sax part up one octave.

So run it thus:

```
% cargo run take5.mid 0,0 0,1 0,3+12
    Finished dev [unoptimized + debuginfo] target(s) in 0.05s
     Running `target/debug/pianoroll take5.mid 0,0 0,1 0,3+12`
MIDI file format: single track
96 MIDI ticks per metronome beat
Copyright: "(C)1991 Roland Corporation"
Tempo: 180 beats per minute
track 0: title: "Take Five"
track 0, channel 0:
        MIDI instrument "Acoustic Grand Piano"
        1394 notes
track 0, channel 1:
        MIDI instrument "Acoustic Bass"
        509 notes
track 0, channel 2:
        MIDI instrument "Electric Guitar (jazz)"
        769 notes
track 0, channel 3:
        MIDI instrument "Alto Sax"
        328 notes
track 0, channel 4:
        MIDI instrument "Pad 2 (warm)"
        no notes
track 0, channel 9:
        Percussion
        1898 notes
track 0, channel 10:
        MIDI instrument "Acoustic Bass"
        539 notes
Writing output to "take5.pdf"
piano roll length: 791.6667 inches
WARNING: exceeding PDF page height limit of 200 inches
```

Note the warning at the end about the page size. PDFs have an unofficial limit of 200x200
inches per page. Technically the standard allows much larger pages, but an older version had this
limit, and many PDF reading software is unable to handle pages larger than 200 inches. Converting
the PDF to PostScript (using the Unix tool `pdf2ps` for example) can help.

Finally, if you open the PDF, you'll notice that even short notes are represented by quite a long
black line. The piano would have to scroll the page unreasonably fast to play it at the right tempo,
so `pianoroll` has a feature build in to compress the page vertically: specify `/<number>` as a
final argument to the program, and it will divide the lengths by that amount. For this song, `/4`
seems to produce a nice result. Do that and you'll get output that ends in:
`piano roll length: 197.91667 inches` which is also nice because it's under the 200-inch soft limit.

## Errors

There are a couple different errors that `pianoroll` can print in various circumstances:

First, and this is the error you'll probably see most often, `pianoroll` will complain if an
instrument tries to play a note while another instrument has that note held down.
There's a little bit of a fudge factor in the code, where if two instruments try to press
the same note close enough to each other (within a fraction of a beat), the error message is
suppressed, based on the assumption that you probably wouldn't be able to hear the difference
anyway. So if you do get this error, you might want to either choose different tracks, or try and
offset one by an octave (+/- 12).
(Maybe in the future I'll hack around this by forcing the first one to stop pressing, but this is
tricky to get right.)

You get a similar error if an instrument tries to release a note that isn't held down. This
shouldn't happen unless your MIDI file is doing something really weird.

You will also get an error if there are notes that go beyond the range of notes representable on a
piano roll. Piano rolls can represent notes from C1 to G7, which is 79 keys. If the MIDI file has
notes outside that range, you can shift the part up or down (see above) and see if it sounds better.

NOTE! Just because there are errors, does not necessarily mean your result will sound bad.
Always listen to the `..._pianoroll.mid` file to check your result first.