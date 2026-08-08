# Full-screen Screens, not a scrolling REPL

Vocab runs in the terminal's alternate screen buffer. The interface is a set of Screens, each of which wholly replaces the last. There is no scrollback and no accumulated log of the session.

Vocab was originally sketched as "kind of like Claude Code" — a scrolling REPL where output builds up above a pinned prompt — so the absence of scrollback is deliberate rather than an oversight. Vocab is used in short bursts to capture or find one Word at a time; what happened earlier in the session has no value once the Word is saved, and the persistent record lives in the database, not the scrollback. Screens also make live-updating search and the launch splash straightforward, both of which fight a REPL.

The cost is that you cannot scroll back to re-read a previous result, and the terminal is restored to its prior contents on exit.
