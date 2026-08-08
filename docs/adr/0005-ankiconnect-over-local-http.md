# AnkiConnect over local HTTP, not MCP

Vocab pushes Cards to Anki through AnkiConnect's own endpoint — `POST http://127.0.0.1:8765`, protocol version 6 — using `createDeck`, `addNote` and `updateNoteFields` against Anki's stock `Basic` note type.

Notably **not** MCP. MCP is a Claude-side mechanism for handing a model tools to call, and there is no model in this loop: the reader types `/sync`, or quits, and a standalone Rust binary talks to a local HTTP server. There is nothing for MCP to attach to. The confusion is worth naming because an Anki MCP server does exist and is the obvious thing to reach for.

The consequence is that Anki has to be running. That is treated as the ordinary state of the machine rather than a failure: the push comes back unavailable, the Word stays queued, the reader is told plainly, and quitting is never delayed by it. The same is true of every other connection or API failure — nothing is ever recorded as synced on the strength of an attempt that didn't land.

Identity is the Anki note identifier Vocab stores per Word, not the Word's spelling, so `allowDuplicate` is set: a card the reader made themselves for the same word is theirs and does not block ours. The sync is one-way, and edits made in Anki are overwritten on the next push of that Word. The one exception is a note *deleted* in Anki — the stored identifier is then permanently stale, so the Word is written back as a new note rather than failing identically on every sync forever. AnkiConnect signals this in prose rather than with a code, so the check reads its wording and is deliberately narrow; anything unrecognised leaves the Word queued, which is the safe direction to be wrong in.

The alternative considered was writing an `.apkg` file for the reader to import. It was rejected because it makes every sync a manual step in the tool the reader was trying not to have to remember, and because a package can add notes but cannot update one — which is exactly what one-note-per-Word needs when a Word gains a Sighting.
