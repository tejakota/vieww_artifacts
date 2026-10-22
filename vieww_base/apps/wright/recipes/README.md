# The Wright recipe gallery

Recipes are tested blueprints with provenance: a person reads what the
project will do in one sentence, installs it with one command, and the
first run happens on the built-in model if no key is set up.

**These files ship UNSIGNED, by design.** The gallery's rule (ADR-028)
is the lesson of the poisoned marketplaces: nothing installs without a
signature. A publisher signs a recipe with their machine's key

```
wright recipe sign recipes/hn-digest.recipe.kdl
```

and prints its public key. Someone who installs it pins that key once —
a signature by a key nobody pinned proves the bytes are intact, not who
made them (ADR-031):

```
wright trust add "Wright gallery" <the publisher's 64-hex public key>
wright recipe install recipes/hn-digest.recipe.kdl
```

while an unsigned one is refused, in words. Treat these files as the
gallery's *source*: the curated, signed copies live where the gallery is
published, pinned to publisher keys you chose to trust.
