# Agent Skill

The release archive and source repository include an Agent skill under `skills/scorekit`. It teaches a skill-capable coding Agent how to query the schema, write scenes, validate musical structure, apply grammar profiles, build audio assets, and compile MakeCode `music.createSong` TypeScript.

Install it together with the local binary:

```bash
make install
```

Install only the skill or choose another skill root:

```bash
make install-skill
make install-skill SKILLS_DIR="$HOME/.codex/skills"
```

The default destination is `~/.agents/skills/scorekit`. The installed skill contains `SKILL.md` and the detailed command and DSL reference.

The bundled `examples/narrative-film-score.md` demonstrates a complete Agent prompt, narrative brief, palette decision, deterministic musical translation, validation/build commands, and completion report. It ships with two deliberately unrelated worked scenes that can be copied into a project and rendered directly: `examples/exile-in-the-dunes.yaml` (solo strings, D minor) and `examples/lantern-keepers.yaml` (music box, G major, 3/4), plus `examples/night-market-strut.yaml`, a GM-only clip groove. The palette is a decision inside the brief rather than a default — `palettes.md` documents the decision protocol and the variation axes, and `scorekit lint --corpus` measures whether the next piece actually differs from the last one.
