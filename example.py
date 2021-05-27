#!/usr/bin/python3
import os
import libcut_ts

if __name__ == '__main__':
    # extract all audio streams from media file
    libcut_ts.split_audios("main.ts",    # input file 
                           "/tmp",       # base dir for output audio files dirs
                           "audio.ts"    # output file name  
                           )

    # cut media file with specific 'seek time' and 'duration time'
    libcut_ts.cut_ts_iframe("main.ts",   # input file
                            "split.ts",  
                            1.5,         # seek time (in second)
                            30           # duration (in second)
                            )

