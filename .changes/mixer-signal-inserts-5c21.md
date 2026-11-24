---
"mixer-view" = "minor"
---

The mixer shows a Signal FX block by block: each phase a bar with its switch, make-up and output level, each block an insert with its status, key source and meter, folding back to one insert; a block opens close up with its parameters, meters, algorithm and moves. Read from `signal.chain/1` through a `SignalSource` the engine installs; edits through `SIGNAL_RUN` and `FX_SET`; `SIGNAL_INSERTS` and `SESSION_CLOSEUP {block}` for agents (session)
