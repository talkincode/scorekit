# Narrative Brief Convention

A Narrative Brief is an upstream creative contract for the Agent that writes a
scorekit scene. It describes **whose experience the music follows, what changes
inside that character, and what the listener must be able to remember**.

It is deliberately **not** a scorekit DSL, schema, or compiler input:

- scorekit never reads or validates it;
- its statements do not have deterministic musical meanings;
- the Composer Agent remains responsible for translating it into concrete
  motif, harmony, rhythm, instrumentation, and performance decisions;
- `scene.yaml` remains the single executable source of truth for scorekit.

This boundary keeps narrative reasoning in the upstream Agent while preserving
scorekit's role as a deterministic music compiler.

## Required narrative decisions

A useful brief answers five questions before any notes are chosen:

1. **World:** What remains true regardless of the protagonist?
2. **Character:** Whose perception organizes the piece?
3. **Desire:** What are they trying to reach, recover, protect, or escape?
4. **Rupture:** What event makes the opening belief impossible to maintain?
5. **Choice:** What do they do at the end that they could not have done at the
   beginning?

Prefer observable contradictions and actions over scalar emotion controls.
For example, write `he continues after hope disappears`, not `hope: 0.2`.

## Motif contract

The brief gives the central musical sentence a dramatic job without prescribing
its notes. Define:

- the question or claim the motif expresses;
- one invariant that makes every return recognizable;
- what is withheld or damaged at the rupture;
- what changes in the final return;
- whether the ending answers the motif or leaves it open.

The Composer Agent may choose the pitches, rhythm, register, harmony, and
instrumentation. Once chosen, however, the invariant must remain audible. A
motif that exists in YAML but cannot be recognized by a listener has not met the
contract.

## Translation record

The produced scene should record the Agent's major translation decisions in
comments: which track carries the protagonist, where the invariant returns,
where the rupture occurs, and how the final statement differs. These comments
explain intent; only the actual DSL fields have compile semantics.

The workflow is:

```text
story material
    -> Narrative Brief (creative contract, not compiled)
    -> Composer/Arranger Agent
    -> scene.yaml (deterministic executable specification)
    -> Score IR -> MIDI -> rendered asset
```

## Listening acceptance

Do not judge the translation by inspecting the YAML. Render it and conduct a
blind listen against the previous version. Without showing the brief, ask:

1. Can the listener describe a person or point of view, not only a place?
2. Is the ending character psychologically different from the opening one?
3. Can the listener hum, tap, or accurately describe one recurring sentence?

Record the answers before revealing the titles or intent. The experiment passes
only if all three answers are supported by details the listener actually heard.
If the brief is clear but the answers remain negative, improve the composition;
do not add narrative fields to the scorekit schema.

## Evidence threshold for a reusable layer

One successful piece proves only that one composition worked. Promote this
convention into a more formal upstream tool only after it survives at least
three materially different stories and more than one Composer Agent. Even then,
the formalization belongs outside the scorekit compiler unless its fields gain
deterministic, testable compile semantics.

See [`examples/briefs/dunes-v4.yaml`](../examples/briefs/dunes-v4.yaml) for a
worked experimental brief and
[`examples/scenes/dunes-v4.yaml`](../examples/scenes/dunes-v4.yaml) for one
translation of that brief.
