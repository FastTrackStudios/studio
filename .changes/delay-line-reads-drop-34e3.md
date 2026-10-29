---
"dsp-core" = "patch"
---

Delay-line reads drop the integer modulo and the small conversion helpers inline across crates: Hall reverbs ~25% cheaper per instance, bit-identical output (session)
