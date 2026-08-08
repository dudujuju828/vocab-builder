#!/usr/bin/env python3
"""Build the bundled dictionary from a Princeton WordNet distribution.

Produces a read-only SQLite database of word senses, per ADR 0001. The output is
committed to the repo and bundled into the binary; it is never fetched at runtime.

    python tools/build_dictionary.py --out assets/wordnet.db

Downloads the WordNet 3.1 dict tarball into a cache directory unless one is
supplied with --tarball. Re-running is idempotent: the output is rebuilt from
scratch each time, so the database is a pure function of the WordNet release.
"""

from __future__ import annotations

import argparse
import os
import sqlite3
import sys
import tarfile
import urllib.request
from pathlib import Path

WORDNET_URL = "https://wordnetcode.princeton.edu/wn3.1.dict.tar.gz"

# WordNet splits adjectives across two synset types: head adjectives ("a") and
# satellite adjectives ("s"). Readers do not care about the distinction.
POS_NAMES = {"n": "noun", "v": "verb", "a": "adjective", "s": "adjective", "r": "adverb"}

# index.<name> and data.<name> pairs. index.adj covers both "a" and "s" synsets.
POS_FILES = ["noun", "verb", "adj", "adv"]


def download(url: str, dest: Path) -> Path:
    if dest.exists():
        print(f"using cached tarball {dest} ({dest.stat().st_size:,} bytes)")
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    print(f"downloading {url}")
    with urllib.request.urlopen(url, timeout=120) as response:
        payload = response.read()
    dest.write_bytes(payload)
    print(f"wrote {dest} ({len(payload):,} bytes)")
    return dest


def extract(tarball: Path, dest: Path) -> Path:
    """Extract the tarball and return the directory holding the index/data files."""
    if not dest.exists():
        print(f"extracting {tarball}")
        with tarfile.open(tarball, "r:gz") as archive:
            archive.extractall(dest)
    for candidate in [dest, *sorted(p for p in dest.rglob("*") if p.is_dir())]:
        if (candidate / "data.noun").exists():
            return candidate
    raise SystemExit(f"no data.noun found under {dest} — is this a WordNet dict archive?")


def parse_data(path: Path) -> dict[int, tuple[str, str]]:
    """Map synset offset -> (pos, definition) from a data.<pos> file.

    A gloss is a definition optionally followed by quoted examples:
        a dwelling that serves as living quarters; "the house was built in 1900"
    Splitting on the first '; "' keeps multi-clause definitions (which use bare
    semicolons) intact while dropping the examples, which nothing displays.
    """
    synsets: dict[int, tuple[str, str]] = {}
    for line in path.read_text(encoding="latin-1").splitlines():
        if line.startswith("  ") or not line.strip():
            continue  # copyright header
        head, _, gloss = line.partition(" | ")
        fields = head.split()
        if len(fields) < 3:
            continue
        offset, ss_type = int(fields[0]), fields[2]
        definition = gloss.strip().partition('; "')[0].strip()
        synsets[offset] = (POS_NAMES.get(ss_type, ss_type), definition)
    return synsets


def parse_index(path: Path) -> list[tuple[str, list[int]]]:
    """Map lemma -> synset offsets from an index.<pos> file, most common sense first.

    Line format, whitespace separated:
        lemma pos synset_cnt p_cnt [ptr_symbol...] sense_cnt tagsense_cnt offset...
    The pointer symbols are variable in number, so the offsets can only be found
    by counting past p_cnt of them.
    """
    entries: list[tuple[str, list[int]]] = []
    for line in path.read_text(encoding="latin-1").splitlines():
        if line.startswith("  ") or not line.strip():
            continue  # copyright header
        fields = line.split()
        if len(fields) < 6:
            continue
        lemma, synset_cnt, p_cnt = fields[0], int(fields[2]), int(fields[3])
        offsets = [int(value) for value in fields[4 + p_cnt + 2 :]]
        if len(offsets) != synset_cnt:
            raise SystemExit(f"{path.name}: malformed entry for {lemma!r}")
        # WordNet joins multi-word lemmas with underscores.
        lemma = lemma.replace("_", " ").lower()
        # WordNet indexes collocations ("hard cash") alongside single words.
        # Lookup is by the exact spelling of one captured Word, so a lemma with a
        # space can never be reached; keeping them would be a third of the rows.
        if " " not in lemma:
            entries.append((lemma, offsets))
    return entries


def build(dict_dir: Path, out: Path) -> None:
    if out.exists():
        out.unlink()
    out.parent.mkdir(parents=True, exist_ok=True)

    connection = sqlite3.connect(out)
    connection.executescript(
        """
        CREATE TABLE synsets (
            id          INTEGER PRIMARY KEY,
            pos         TEXT NOT NULL,
            definition  TEXT NOT NULL
        );
        CREATE TABLE senses (
            word       TEXT NOT NULL,
            synset_id  INTEGER NOT NULL REFERENCES synsets(id),
            sense_num  INTEGER NOT NULL
        );
        """
    )

    # Synset offsets are only unique within a part of speech, so they are rebased
    # onto a single id space shared by both tables.
    next_id = 1
    sense_rows: list[tuple[str, int, int]] = []
    synset_rows: list[tuple[int, str, str]] = []

    for name in POS_FILES:
        synsets = parse_data(dict_dir / f"data.{name}")
        ids: dict[int, int] = {}
        for offset, (pos, definition) in synsets.items():
            ids[offset] = next_id
            synset_rows.append((next_id, pos, definition))
            next_id += 1
        for lemma, offsets in parse_index(dict_dir / f"index.{name}"):
            for sense_num, offset in enumerate(offsets, start=1):
                if offset in ids:
                    sense_rows.append((lemma, ids[offset], sense_num))
        print(f"  {name}: {len(synsets):,} synsets")

    # Dropping collocations orphans every synset reachable only through one.
    reachable = {synset_id for _, synset_id, _ in sense_rows}
    synset_rows = [row for row in synset_rows if row[0] in reachable]

    connection.executemany("INSERT INTO synsets VALUES (?, ?, ?)", synset_rows)
    connection.executemany("INSERT INTO senses VALUES (?, ?, ?)", sense_rows)
    # The only query the tool makes is lookup by exact lowercased spelling.
    connection.execute("CREATE INDEX senses_by_word ON senses(word)")
    connection.commit()
    connection.execute("VACUUM")
    connection.close()

    print(f"\n{len(synset_rows):,} synsets, {len(sense_rows):,} senses")
    print(f"wrote {out} ({out.stat().st_size:,} bytes)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("assets/wordnet.db"))
    parser.add_argument("--tarball", type=Path, help="a local wn*.dict.tar.gz")
    parser.add_argument(
        "--cache",
        type=Path,
        default=Path(os.environ.get("TEMP", "/tmp")) / "vocab-wordnet",
        help="where to download and unpack WordNet",
    )
    args = parser.parse_args()

    tarball = args.tarball or download(WORDNET_URL, args.cache / "wn3.1.dict.tar.gz")
    dict_dir = extract(tarball, args.cache / "dict")
    print(f"building from {dict_dir}")
    build(dict_dir, args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
