# Theme file save concurrency

The current editor saves a selected role to an existing layer. It compares the
file's bytes with those observed when its draft began, then writes the draft.
The host serializes its own load, edit and save operations, and refuses a save
when its displayed version changed. A disk change visible at the comparison is
reported as a conflict; the draft remains available. Other drafts are not
saved. Compilation and file I/O run on the editor worker.

This is an optimistic check, not an atomic compare-and-swap: an external writer
can change the file after the comparison and before or during the write. Frame
may overwrite that change, and an interrupted write may leave a partial file.
Users must avoid concurrent external writes to the target while saving. A
last-good in-memory theme does not make a partially written file durable. The
current API does not offer a guarantee against arbitrary external writers.

## Design for coordinated writers

A stronger protocol requires every writer of a layer to cooperate. Before
reading, validating or replacing a file, each writer must acquire the same
exclusive OS advisory lock on a stable sidecar associated with the canonical
layer path. The sidecar must not be unlinked or replaced while it can be used:
locking the theme file itself is insufficient because rename changes its inode.
The lock service must work across the processes and filesystem involved; where
that cannot be established, the writer must refuse a protected save rather than
claim exclusion. Aliases, symlinks, case normalization and non-local filesystems
need an agreed identity and lock domain.

While holding the lock, a writer reads the current bytes and compares them to
its expected base. On a mismatch it returns a conflict and retains its draft.
On a match it writes a unique temporary file in the same directory, preserves
appropriate permissions, flushes the file, atomically replaces the target, and
flushes the directory where the platform supports that durability guarantee.
It then updates its observed baseline and releases the lock. Errors before the
replacement leave the target intact; errors after replacement must be reported
as an uncertain durability outcome, not as an unapplied save. Host version
validation and publication must remain serialized with this operation.

This protocol is a design, not implemented by the current revision. Advisory
locks cannot stop an editor that ignores them, and no portable filesystem API
provides a byte-content compare-and-swap against such a writer. For a strict
no-overwrite guarantee with uncooperative editors, write to a new, exclusively
created output file and require a separate, explicitly coordinated promotion.

Implementation acceptance tests must run two processes using the same sidecar:
force writer B to attempt a save between A's comparison and replacement, verify
B cannot enter that interval, and verify it sees a conflict after A commits.
Also test process exit while holding the lock, aliases, failed temporary writes,
replacement failure, reload, and retained drafts. Platform and filesystem support
must be tested rather than inferred from the advisory-lock API.
