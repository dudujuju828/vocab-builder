# Bundled offline dictionary from WordNet

Definitions are served from a SQLite database bundled with the tool, prebuilt from Princeton WordNet — not fetched from a dictionary API at lookup time.

Vocab is used mid-reading, so a lookup must return instantly and must work without a network (trains, planes). An online API adds per-lookup latency, rate limits, and usually an API key to manage. WordNet was chosen over a Wiktionary extract because it is small, clean, and already structured into senses; the Wiktionary route is more current but needs a multi-gigabyte dump and a parsing pipeline for coverage we don't need.

The cost is staleness — WordNet is effectively frozen around 2011, so recent coinages will be missing, and its glosses are terse. The AI layer covers both gaps.
