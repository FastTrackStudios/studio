# Talk notes: "How to write the perfect function" — Logan Smith

Source: <https://www.youtube.com/watch?v=2OMRWPOSw9s> (Logan Smith, the
`_noisecode` channel; ~47 min, examples in C++). Notes are a paraphrase
taken from the auto-generated transcript, not a transcript. Timestamps
are approximate. The video's description links the talks it builds on:
Tony Van Eerd's talk (the "write the functions you want to see in the
world" parable), Sean Parent's "No Raw Loops", and Uncle Bob's Single
Responsibility post.

What the talk does **not** cover: line-count limits, naming
conventions in general, argument-count rules, or comment style. Those
parts of SKILL.md are this repo's rules, not the speaker's.

## 0:00 — The parable

A junior asks how to read the config file; the mentor says "there's a
function for that" (`get_config_file`, `get_config_file_path`). The
code does not compile — those functions do not exist yet; the junior has
to write them. The point: the junior already *knew* which functions
ought to exist, at the right level, easy to snap together. The talk
tries to say what makes a function feel like that.

## 1:50 — Why functions at all

Functions are an artificial abstraction the compiler works hard to give
you; the CPU only knows instructions and jumps. Reasons we use them
anyway:

- **Local reasoning** — a well-built function is a sandbox: you can
  understand its body without reading the rest of the program, and
  changes elsewhere cannot reach in.
- **Abstraction** — the flip side: once written, you put a name tag on
  it and use it as a brick without zooming in.
- **Testability** — a named, isolated piece of behaviour can be called
  from a test as easily as from the system.
- **Reuse** — real, but not the main reason. "Extract after n
  repetitions" — his answer for n is *one*: consider extracting code the
  first time you write it, for the three reasons above.

None of this adds power; a perfect mind could write goto soup. Functions
exist because people (and agents) can hold little in their heads —
they are a tool for managing complexity.

## 7:29 — Pure vs impure, and why that split is not enough

The usual taxonomy: **pure** (deterministic, reads no mutable globals,
mutates nothing visible) vs **impure** (depends on its environment, may
mutate anything, does the I/O). Pure gives local reasoning and
testability; impure gives neither.

**From ~9:39 (the part the link points at):** he argues that the
pure/impure line is drawn in the wrong place:

- An in-place `sort` mutates its argument — impure by the textbook — yet
  it is deterministic, locally reasonable and trivially testable.
- A `get_time()` mutates nothing and takes nothing, yet you have to
  "look at your watch" to know what it returns. Not testable, no local
  reasoning.

So he replaces the axis:

- **Honest** functions touch the outside world *only through their
  signature*: they read and write nothing the caller did not hand them.
  The caller can fully control them by choosing the arguments. Covers
  most pure functions *and* in-place `sort`, which only mutates memory
  you explicitly lent it.
- **Dishonest** functions' signatures are not the whole story of their
  inputs and outputs (globals, clocks, singletons, I/O). They still give
  abstraction and reuse, but forfeit local reasoning and testability.

Key property: **dishonesty is infectious**. An honest function cannot
call a dishonest one and stay honest. So honest functions can only live
at the leaves of the call tree; dishonest ones sit near the root.

## 12:50 — "Species" of functions

Honest:
- the true pure function (inputs → output, nothing mutated); a method
  is the same thing with an invisible `self` parameter;
- the pure accessor (returns a reference/copy of a field);
- the honest mutator, e.g. `Vec::clear` — mutates its input, but the
  resulting state is fully determined; it "returns" through the input;
- the two-channel mutator, e.g. C++ `remove_if` — mutates the input *and*
  returns what it did, so the caller can carry on.

Dishonest:
- the **dishonest accessor** — named like a getter, but filters through
  global state (his example: "required assets" that silently drops the
  ones some global asset manager already has loaded). It passed his
  tests and failed later for someone else, because its result depended
  on what had happened elsewhere first. The name promised one thing; a
  hidden assumption of the original caller was baked in.
- I/O like `draw_to_screen` — essential (it is the point of the
  program), but "beautiful with teeth": it must be handled with
  discipline because it spreads.

A third genus, **"backwards" functions** — framework hooks such as
`update()` or `main()`: they abstract the *call site* away from the
implementation rather than the reverse. Keep their bodies tiny; jump
back into your own functions straight away. Their single job is glue.

## 18:05 — Guideline 1: build from honest functions, inject dishonesty at the top

Build the system's logic from honest functions and add the dishonest
parts at the highest possible level — honest core, dishonest skin.

- Permutations example: a `main` that reads input, sorts, and prints
  each permutation in a loop cannot be unit-tested. Pull the honest part
  into `all_permutations(s) -> Vec<String>`; `main` does only I/O. Now the
  skin is swappable (console vs file I/O — "not an interesting or
  salient part of our program").
- The usual shape of this refactor: instead of acting on each result as
  you find it, **build a value that represents the whole result** and let
  dishonest code consume it.
- If materialising is too costly (n! permutations), **inject the
  action**: `for_each_permutation(s, f)`, and build the collecting
  version on top of it. In Rust, he notes, the natural form is a custom
  **iterator**.
- PRNG example (particle `populate`): reading a global seeded generator
  makes the function dishonest and tied to every other user of that
  global (who reseeded it? another thread advanced it? someone deleted
  the seed call?). Fix: take the generator as a parameter. The caller
  creates a local one seeded from the clock — or from a fixed seed for
  reproducible tests, QA, debugging, shareable seeds. Passing the global
  in still works, so nothing is lost; baking it in loses the option
  forever. A PRNG itself is the model: honest state machine, dishonest
  only at the seed.

## 25:50 — Guideline 2: empathetic signatures

The signature is read first by the human calling it; be nice to them.

- **Opaque call sites**: six positional args ending in a bare `bool`
  nobody can guess. Fix with a parameter struct built just for this
  call (readable call site, easier to add fields), or strong types per
  argument (more boilerplate).
- Distinct anti-pattern: demanding a whole big struct when you need two
  fields of it — "hand me your whole wallet" when the cashier needs a
  card. Opens dangerous possibilities and excludes callers who do not
  have that struct.
- **Do not over-constrain parameters.** Taking `std::vector<int>` forces
  the caller's storage; a `span` accepts any contiguous storage; if the
  function only iterates, ask only for something iterable.
- **But ask for what you really need.** If B must be called after A,
  have A return a token (a "receipt") that B takes. Mutex example:
  instead of a comment "lock must be held", B takes the lock guard — it
  becomes impossible to call B unlocked, and the classic unnamed-guard
  bug (dropped immediately) stops compiling.
- "Write the function you wish existed": mid-thought in the particle
  update he needs bounce maths he does not remember, so he calls a
  not-yet-written `bounce(velocity, surface)` and finishes the current
  thought. A function call abstracts details in *time* too.

## 31:00 — Encoding invariants in types

`bounce` needs a unit-length normal. Options: normalise inside (but
normalising can fail on a zero vector — now there is an error case),
leave it to the caller (NaN hunting), assert, or return an optional
(pessimises the happy path). His answer: the parameter type is wrong.
Take a `NormalizedVec3` — a thin wrapper that can only be constructed
normalised, converts freely back to a plain vector. Responsibility moves
to the caller, who knows more (the surface may already store a
normalised normal; normalise once and reuse). The invariant also lets
you specialise: `normalize` on an already-normalised type is a no-op;
cross product of two normals is a normal. There is a line past which
encoding every pre/post-condition in types gets silly; he admits he
"knows it when he sees it".

## 36:40 — Guideline 3: one level of abstraction (the golden rule)

Bodies: he cites single responsibility (one reason to change), Tony Van
Eerd (separate producing data from acting on it — like honest vs
dishonest), and Sean Parent (non-trivial loop bodies become functions,
better yet standard algorithms). His rule claims to cover all three:
**every line of a function body sits at the same level of abstraction.**
Writing a function is stacking bricks; if you need to zoom into one
brick, fiddle, and zoom back out, that zoom is a function call. Never
zoom into more than one brick in one function.

Worked example: `asset_has_type(name, ty)` that

1. lower-cases the name with a hand-written char loop,
2. hand-rolls a binary search over a sorted vector (which, he notes
   while recording, has a bug),
3. aborts the program if not found,
4. compares the type.

Tells: section comments labelling the steps are a sign of zooming in and
out. Outline it as bullets; group bullets at the same indentation —
each group is a function. Result: `find_asset(name)` (= `to_lower` +
search) and the top function = find + compare type; return a `bool`, or a
richer result to tell "wrong type" from "not found", instead of
aborting. Using the library's `lower_bound` puts a little "gore" back
but stays at one level: lines need not each be a single call, but do not
open the algorithm's hood inline.

Final step: the lower-case-then-binary-search pairing is an **ad hoc data
structure** smeared across the class (every insert must mirror it).
Recognise it and encapsulate it — a case-insensitive map (he names maps
`value_by_key`, e.g. `assets_by_name`). Now the original function is a
one-line lookup and it is obviously correct "assuming all of its bricks
do".

## 46:10 — His summary

1. Keep the system honest so it can be reasoned about locally and tested;
   inject the outside world at the very top.
2. Signatures should tell callers useful things about inputs and outputs,
   without over-constraining parameters.
3. A function body stays at one level of abstraction.
