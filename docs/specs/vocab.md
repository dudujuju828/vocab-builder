# Spec: Vocab

## Problem Statement

I read a lot, and I regularly meet words I don't properly know. Right now the whole interaction is: notice the word, look it up, understand the sentence, keep reading, forget it. The next time I meet the same word — possibly in a different book, months later — it feels vaguely familiar and I skim past it, still not knowing it. Nothing accumulates.

Two things are missing. There is no record: I can't answer "have I looked this up before?", and I have no way to see that a word keeps recurring. And there is no retention: a single lookup, read once and closed, doesn't stick.

Anki already solves the retention half well, but making a good card mid-page is enough friction that I don't do it. A bare word–definition pair is also a poor card — the sentence I actually met the word in is what makes the meaning concrete, and that context is exactly what gets lost between noticing the word and sitting down with Anki later.

## Solution

`vocab` is a terminal tool, launched by typing `vocab`. It opens full-screen with an ASCII splash and stays out of the way.

You tell it which Book you're reading once; that Current Book persists across launches. From then on, capturing a Word is: type the word, type the sentence you met it in. The tool attributes it to the Current Book, dates it, and shows you the Definition from a bundled offline dictionary immediately — no network, no wait.

Meeting the same Word again doesn't create a duplicate. It adds a Sighting: a second sentence, a second Book, a second date, hanging off the one Word. Two sightings of the same word is a signal, not a mess — it means the word matters and you still didn't know it.

Typing anything that isn't a command searches everything you've captured, updating as you type. It matches the Words themselves, and also the sentences and Book names, so you can find a word you've forgotten the spelling of by the phrase around it.

Behind the offline core, two things reach out. Shortly after each capture, an AI Note is written — a short reading of what that Word is doing in that particular sentence, which the terse dictionary Definition doesn't give you. And the Words sync to Anki as cards, so the retention half happens in the tool that's already good at it.

## User Stories

### Launching and orienting

1. As a reader, I want to launch the tool by typing `vocab` and nothing else, so that capturing a word costs me almost no effort mid-page.
2. As a reader, I want the tool to open in under a tenth of a second, so that it feels like a tool rather than an application I'm waiting on.
3. As a reader, I want to see ASCII art of the word "vocab" when it launches, so that the tool has a character of its own.
4. As a reader, I want to see which Book I'm currently reading on the opening screen, so that I know what my next capture will be attributed to without having to check.
5. As a reader, I want a `/help` command listing every command and what arguments it takes, so that I don't have to remember the surface.
6. As a reader, I want to see argument hints as I type a command, so that I learn the surface by using it rather than by reading help.
7. As a reader, I want `/quit` to exit and restore my terminal to what was on it before, so that the tool leaves no mess behind.

### The Library and the Current Book

8. As a reader, I want to add a Book to my Library by name, so that captures can be attributed to it.
9. As a reader, I want to switch my Current Book with `/book`, so that when I start a new book my captures follow me.
10. As a reader, I want `/library` to show me every Book I've added, so that I can see what I've been reading and pick from it.
11. As a reader, I want to see how many Words I've captured from each Book in my Library, so that I can tell which books are stretching me.
12. As a reader, I want my Current Book to persist across launches, so that I set it once per book rather than once per session.
13. As a reader, I want switching to a Book that isn't in my Library yet to offer to add it, so that starting a new book is one step rather than two.

### Capturing a Word

14. As a reader, I want to capture a Word with `/add <word>`, so that the command is short enough to type dozens of times without resenting it.
15. As a reader, I want to be prompted for the sentence after giving the word, so that I'm not composing one long command with quoting rules.
16. As a reader, I want to paste the sentence rather than retype it when I'm reading on a screen, so that capture stays cheap.
17. As a reader, I want the Book attributed automatically from my Current Book, so that I never type the Book's name more than once.
18. As a reader, I want the date recorded automatically, so that I can later see when I met a word.
19. As a reader, I want to see the Definition immediately after capturing, so that the capture and the lookup are one action rather than two.
20. As a reader, I want to be able to cancel part-way through a capture, so that a mistyped word doesn't force me to save something wrong.
21. As a reader, I want to capture a Word that isn't in the bundled dictionary, so that proper nouns, jargon, and recent coinages aren't silently rejected — the Sighting is still worth keeping.
22. As a reader, I want to be told clearly when there's no Definition rather than shown an empty panel, so that I know it's a gap in the dictionary and not a bug.

### Meeting a Word again

23. As a reader, when I capture a Word I already have, I want to be told I already have it and shown where I met it before, so that I get the "I have seen this" signal the tool exists to give me.
24. As a reader, I want to be asked whether to add another Sighting for that Word, so that the choice is mine rather than the tool's.
25. As a reader, when I say yes, I want a new Sighting attached to the same Word, so that I never end up with the same Word twice.
26. As a reader, when I say no, I want the existing Word left exactly as it was, so that declining is safe.
27. As a reader, I want a Word with several Sightings to appear once in search results, so that recurrence enriches the Word rather than cluttering the list.

### Searching

28. As a reader, I want to search by typing plain text with no command prefix, so that the most common action is the cheapest one.
29. As a reader, I want results to update on every keystroke, so that I can stop typing the moment I see what I wanted.
30. As a reader, I want fuzzy matching, so that a half-remembered or misspelled word still finds the Word.
31. As a reader, I want Word matches ranked above sentence and Book matches, so that the obvious result is always at the top.
32. As a reader, I want my search to also match the sentences I captured, so that I can find a word I can't spell by the phrase around it.
33. As a reader, I want my search to also match Book names, so that I can pull up everything I captured from one book.
34. As a reader, I want to see which kind of match each result is, so that a sentence hit isn't mistaken for a word hit.
35. As a reader, I want to open a result and land on that Word's detail, so that searching flows into reading.
36. As a reader, I want a clear empty state when nothing matches, so that I can tell "I've never captured this" from "the search is broken" — and that answer is itself useful.

### Reading a Word back

37. As a reader, I want a Word's detail screen to show its Definition, so that the primary answer is always there.
38. As a reader, I want to see every Sighting of that Word, so that I can see the range of ways I've met it.
39. As a reader, I want each Sighting to show its sentence, its Book, and its date, so that each encounter is anchored in context.
40. As a reader, I want Sightings ordered with the most recent first, so that the freshest Sighting leads.
41. As a reader, I want to return to where I came from, so that browsing doesn't strand me.

### AI Notes

42. As a reader, I want a Note written automatically after each capture, so that I get the contextual reading without asking for it.
43. As a reader, I want capture to return immediately rather than waiting on the Note, so that the tool stays fast and works with no network.
44. As a reader, I want to see that a Note is still pending rather than an empty space, so that I don't think it failed.
45. As a reader, I want the Note to appear when it's ready without me reloading anything, so that it feels like it arrived rather than like I fetched it.
46. As a reader, I want `/explain` to write or rewrite the Note for the Sighting I'm looking at, so that I can ask again when the first answer was weak.
47. As a reader, I want Notes to queue when I'm offline and be written when I'm next connected, so that reading on a train costs me nothing.
48. As a reader, I want a failed Note to leave the Sighting completely intact, so that the network can never cost me a capture.
49. As a reader, I want to see that a Note failed and be able to retry it, so that failures are visible and recoverable rather than silent.
50. As a reader, I want the Note presented as a second opinion beside the Definition, not instead of it, so that I can tell the dictionary from the model.

### Anki sync

51. As a reader, I want `/sync` to push everything not yet synced to Anki, so that my captures become cards I actually review.
52. As a reader, I want one Anki note per Word rather than per Sighting, so that recurrence enriches a card instead of creating duplicates.
53. As a reader, I want the card's back to carry the Definition and every Sighting's sentence, Book, and Note, so that the card has the context that makes the word stick.
54. As a reader, I want a new Sighting on an already-synced Word to update its existing card, so that cards stay current without multiplying.
55. As a reader, I want a sync to run automatically when I exit, so that I don't have to remember to do it.
56. As a reader, I want a way to exit without syncing, so that I can leave quickly when I want to.
57. As a reader, when Anki isn't running at exit, I want the exit to be instant and the Words left queued, so that syncing can never hold my quit hostage.
58. As a reader, I want to be told what synced and what didn't, so that I trust the state of my deck.
59. As a reader, I want the sync target deck to be configurable, so that vocab cards land where I want them.

### Working offline

60. As a reader, I want capture, Definition lookup, search, and browsing to work with no network at all, so that the tool is usable on a plane or a train.
61. As a reader, I want the dictionary to ship with the tool rather than be downloaded on first run, so that there's no setup step between installing and using it.
62. As a reader, I want my captured Words stored outside the working directory, so that `vocab` behaves the same from any folder.

## Implementation Decisions

### Shape of the application

The tool is a single Rust binary using Ratatui, per ADR 0002. It runs in the terminal's alternate screen buffer and presents a set of Screens, each of which wholly replaces the last, per ADR 0003. There is no scrollback and no accumulated session log.

The Screens are: **Home** (splash, Current Book, prompt), **Search** (live results), **Word** (Definition and all Sightings), **Library** (Books, with capture counts), and **Help** (the command surface). Capture is a prompt sequence rather than a Screen of its own — it collects the word, then the sentence, then shows the result on the Word screen.

The input line is present on every Screen. Text with no leading slash is a search; text with a leading slash is a command. The commands are `/add`, `/book`, `/library`, `/explain`, `/sync`, `/help`, `/quit`. As the user types a command, the argument hint for the matching command is displayed inline.

### Storage

A single SQLite database holds user data, accessed through `rusqlite`. It lives in the OS application-data directory rather than the working directory, so the tool behaves identically regardless of where it is invoked from.

The schema covers:

- **Books** — name, creation date.
- **Words** — spelling, unique. A Word is never duplicated; this is enforced by the schema, not by application logic.
- **Sightings** — belongs to one Word and one Book; holds the sentence, the capture date, the Note, and the Note's state.
- **Application state** — the Current Book, as a single persisted row.
- **Sync state** — per Word, the identifier of its corresponding Anki note (absent until first synced) and whether it has changed since it was last pushed.

A Note has three states: **pending** (queued, not yet written), **ready** (written), and **failed** (attempted and errored, retryable). The state is stored rather than inferred, so a Note that was pending when the process exited is still pending when it starts again.

### Dictionary

Definitions come from a WordNet-derived SQLite database bundled with the binary, per ADR 0001. It is read-only and separate from the user database, so it can be replaced wholesale by a new build without touching user data. Lookups are by exact spelling, case-insensitively; a Word may have several Definitions and each carries its part of speech. A miss is a normal outcome, not an error — the Sighting is stored regardless.

### Search

Search is backed by `nucleo`, matching over three fields: Word spellings, Sighting sentences, and Book names. Results are grouped by Word so a Word with several Sightings appears once. Ranking places Word matches above sentence and Book matches; within a band, the matcher's own score orders the results. Each result carries which field it matched on so the UI can show it. The corpus is small enough — thousands of rows — that the whole search runs against memory on each keystroke with no incremental index.

### AI Notes

Note generation sits behind a **`NoteWriter`** interface: given a Word, its sentence, and its Book, return a short reading of the Word in that Sighting. The production implementation calls DeepSeek per ADR 0004.

Capture never blocks on it. `/add` writes the Sighting with a pending Note and returns; a background task on `tokio` picks up pending Notes and writes them. When one completes, the Word screen updates in place if it is showing that Sighting. `/explain` enqueues the currently-displayed Sighting, overwriting any existing Note — this is the one path that can replace a Note that already succeeded.

If a Note attempt fails, it is marked failed and the Sighting is untouched. Pending Notes survive process exit and are picked up on next launch, which is what makes offline capture work: reading on a plane leaves a queue that drains when the network returns.

The prompt is constructed instructions-first, with the variable word/sentence/book tail last, so the fixed prefix hits DeepSeek's context cache. The API key comes from the environment; its absence disables Note generation with a visible message rather than erroring on every capture.

### Anki sync

Sync sits behind a **`CardSync`** interface: given a Word with its Definition and all its Sightings, create or update the corresponding Anki note and return its identifier. The production implementation speaks AnkiConnect over HTTP on the local machine — notably *not* MCP, which is not available to a standalone binary.

The mapping is one Anki note per Word. The front is the Word. The back carries the Definition and, for each Sighting, its sentence, its Book, its date, and its Note. A Word that gains a Sighting is marked changed and updates its existing note on next sync rather than creating a second one.

Sync is one-way: `vocab` to Anki. Edits made in Anki are overwritten on the next push of that Word. The destination deck is configurable and defaults to `Vocab`.

`/sync` pushes every changed Word. Exit also triggers a sync unless the user exits explicitly without one. Anki not running is an expected condition, not an error: the sync is skipped, the Words stay queued, and exit is not delayed. Sync failure must never block quitting.

### Configuration

Configuration is a file in the OS configuration directory, covering at minimum the Anki deck name and whether to sync on exit. Secrets are not stored in it — the DeepSeek key is read from the environment.

## Testing Decisions

### What makes a good test here

A good test drives the application the way a reader does and asserts what a reader sees. It sends key events and asserts against the rendered screen. It does not call internal functions, inspect application state, or assert on database rows. If the search ranking is reimplemented, or the Word screen is relaid out, or `nucleo` is swapped for something else, a good test either still passes or fails for a reason a user would care about.

Concretely: a test for capture types `/add`, then a word, then a sentence, and asserts the Definition text appears on screen. It does not assert that a row landed in the `sightings` table.

### The seam

There is one seam. The application is constructed with its dependencies injected, driven with synthetic key events, and rendered through Ratatui's `TestBackend` to an in-memory buffer that the test asserts against.

Three dependencies are supplied differently under test, and only one category is faked:

- **User database** — a real SQLite database in a temporary location, fresh per test. Not faked; the real storage layer is under test.
- **Dictionary** — the real bundled WordNet database. It is read-only and deterministic, so faking it would only test the fake. Tests assert against actual WordNet definitions.
- **`NoteWriter` and `CardSync`** — the only fakes. Test implementations return canned Notes and record the Anki notes they were asked to write. No test touches the network.

### Asynchrony

Because Notes are written in the background, a naive screen assertion races the background task. Tests run the async runtime under deterministic control and drive pending work to completion before asserting, rather than sleeping or polling. This is what keeps the Note flow testable through the single screen seam instead of needing a second, lower seam onto the note pipeline.

### What gets tested

Every user-visible flow through the one seam: capture including the duplicate-Sighting prompt and both answers, Definition display including the not-found case, live search across all three matched fields and the ranking between them, Current Book persistence across a restart, Word detail with multiple Sightings, the Note lifecycle through pending to ready and to failed, `/explain` overwriting an existing Note, and sync behaviour including one-note-per-Word, update-on-new-Sighting, and Anki being unavailable.

The `CardSync` fake is asserted against directly for sync content — that the note the tool tried to write carries the right Definition, sentences, Books, and Notes — since that payload is not visible on screen.

### Prior art

None. This is a greenfield repository, so these tests establish the pattern. The first test written should be the capture flow end to end, because it exercises the whole seam and everything after it follows the same shape.

## Out of Scope

- **Spaced repetition inside `vocab`.** Anki owns scheduling entirely. There is no `/review`, no due dates, no ease factors, and no review state in the schema.
- **Two-way sync with Anki.** Edits made to a card in Anki are not read back and are overwritten. Anki is a destination, not a peer.
- **Automated capture** — clipboard watching, ebook reader integration, OCR, or browser extensions. The sentence is typed or pasted by hand.
- **Editing or deleting captured data.** There is no way to fix a typo in a stored sentence, correct a misspelled Word, or delete a Sighting. This is a real gap and likely the first follow-up, but it is not in this spec.
- **Multiple users, machines, or devices.** One person, one machine, one local database. No account, no cloud sync, no mobile.
- **Languages other than English.** WordNet is English-only and the tool assumes it.
- **Configurable theming.** The old-school look is the look.
- **Scrollback.** Deliberately excluded per ADR 0003 — previous screens are gone, and the durable record is the database.

## Further Notes

**Build order.** v1 is the offline loop with no network dependency at all: splash, Library and Current Book, capture with sentence, WordNet Definition, live search, Word detail. v2 adds AI Notes. v3 adds Anki sync. All three are being built; the phasing exists so the offline core can be used against real reading before anything is layered on it.

**The main risk is capture friction, and v1 is the test of it.** Typing a word and a sentence per unknown word is roughly fifteen seconds and a context switch out of reading. If that turns out to be too much, no amount of AI or Anki integration downstream will save the tool — people stop capturing and the database stops growing. Making the Book ambient removes the largest repeated cost, but v1 should be used against a real book for a week before v2 begins.

**Retention is deferred until v3.** The problem statement is half about not remembering words, and nothing in v1 or v2 addresses that. This is a deliberate sequencing choice, not an oversight, but it does mean the tool does not deliver its motivating benefit until the third phase.

**DeepSeek pricing is expected to rise**, per their own documentation. ADR 0004 records the cost reasoning that led to choosing it; the `NoteWriter` interface and the OpenAI-compatible endpoint mean switching providers is a small change if the economics move.

**Dictionary size is unverified.** WordNet is small, but the actual size of the bundled database and its effect on binary size should be measured before the build pipeline is settled.
