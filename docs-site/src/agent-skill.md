# Agent Skill

The release archive and source repository include an Agent skill under `skills/scorekit`. It teaches a skill-capable coding Agent how to query the schema, write scenes, validate musical structure, apply grammar profiles, and build assets.

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

The bundled `examples/narrative-film-score.md` demonstrates a complete Agent prompt, narrative brief, deterministic musical translation, validation/build commands, and completion report. Its companion `examples/exile-in-the-dunes.yaml` is a schema-validated 24-bar scene that can be copied into a project and rendered directly.
