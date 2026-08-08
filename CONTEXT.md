# Vocab

A terminal tool for capturing words you don't know as you read, together with the book and sentence you met them in, so they can be looked up later and reviewed until they stick.

## Language

**Word**:
A single lexical item you didn't know, unique by spelling. Never duplicated — a second encounter adds a Sighting rather than a second Word.
_Avoid_: term, entry, vocab item

**Sighting**:
One encounter with a Word: the sentence it appeared in, the Book it came from, and the date. A Word has one or more.
_Avoid_: entry, instance, occurrence, context

**Definition**:
What the bundled dictionary says a Word means, independent of where you met it. Belongs to the Word.
_Avoid_: gloss (WordNet's own name for these), meaning, sense

**Note**:
The AI's short reading of what a Word is doing in one particular Sighting's sentence. Belongs to the Sighting, not the Word, and is a second opinion — never a replacement for the Definition.
_Avoid_: gloss, explanation, commentary, AI definition

**Book**:
A source you are reading and capturing Words from.
_Avoid_: source, text, title

**Library**:
The collection of Books known to the tool.

**Current Book**:
The Book you are reading right now. Persists across launches, and every new Sighting is attributed to it until you switch.
_Avoid_: active book, selected book

**Card**:
A Word as it exists in Anki — one per Word, never one per Sighting. Its front is the Word; its back carries the Definition and every Sighting. Anki's own name for what the tool writes is a *note*, which is why the identifier stored against a Word is an Anki note identifier; here the thing itself is a Card.
_Avoid_: flashcard, entry, note (which is Anki's word for this, and ours for something else entirely)

**Deck**:
Where Cards land in Anki. Configurable, and `Vocab` unless you say otherwise.

**Sync**:
Sending every Word whose Card is out of date to Anki. One-way — Vocab to Anki, never back — and it happens on `/sync` and on the way out. One Card within a Sync is *pushed*; the operation as a whole is a Sync and never a push.
_Avoid_: upload, export
