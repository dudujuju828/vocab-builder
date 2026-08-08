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
