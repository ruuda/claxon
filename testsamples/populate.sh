#!/bin/sh

# "I Have a Dream" by Martin Luther King in 1963, 16 bit mono, 22.05 kHz.
if [ ! -f p0.flac ]; then
    curl -Lo p0.flac https://archive.org/download/MLKDream/MLKDream.flac
fi

# Intro of Karl Denson’s Tiny Universe live in 2015, 24 bit stereo, 96 kHz.
if [ ! -f p1.flac ]; then
    curl -Lo p1.flac https://archive.org/download/kdtu2015-01-07.cmc641.flac24/kdtu2015-01-07.cmc641-t01.flac
fi

# "Disarm" by Smashing Pumpkins live at Lowlands 1993, 16 bit stereo, 44.1 kHz.
if [ ! -f p2.flac ]; then
    curl -Lo p2.flac https://archive.org/download/tsp1993-08-07.flac16/tsp1993-08-07d2t01.flac
fi

# "Lowlands" by The Gourds live at Lowlands 2004, 16 bit stereo, 44.1 kHz.
if [ ! -f p3.flac ]; then
    curl -Lo p3.flac https://archive.org/download/gds2004-10-16.matrix.flac/gds10-16-2004d2t10.flac
fi

# "Once Upon a Time" by Smashing Pumpkins live at Pinkpop 1998, 16 bit stereo, 44.1 kHz.
# (The frequency spectrum does not exceed 16 kHz, so this might be a re-encoded mp3,
# but it is a perfectly valid FLAC file nonetheless.)
if [ ! -f p4.flac ]; then
    curl -Lo p4.flac https://archive.org/download/tsp1998-06-01.flac16/tsp1998-06-01t02.flac
fi
