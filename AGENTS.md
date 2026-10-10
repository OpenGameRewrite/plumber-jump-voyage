# Plumber Jump Voyage

This standalone demo is separate from rewrite-engine, its modules and tracker.
Use a codex/ branch. Keep changes here. READMEs address players, not agents.

Only original source and public documentation may be committed.
Never commit ROMs, keys, extracted assets, SDK files, raw disassembly,
decompiler output or downloaded game code, including in private repositories.
The whitelist .gitignore is intentional. Check every staged path before commits.
Local assets and experiments belong in ignored assets/ and local/ directories.
Local extraction may invoke separately installed tools with the user's own ROM
and supplied prod.keys/title.keys. Never bundle decryption tools or keys, print
key contents, or obtain ROMs/keys from third parties. Extraction wrappers must
require the user's local inputs and keep all output ignored.

Use Bevy directly. Keep one movement controller and one collision scene.
Do not invent a shared engine or copy OMM code. Describe fidelity limits honestly.
Run cargo fmt, cargo check, headless checks, and a graphical smoke run before shipping.
