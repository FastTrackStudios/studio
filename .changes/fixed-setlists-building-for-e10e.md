---
"setlists" = "patch"
---

Fixed setlists building for iOS: the engraver's PDF writer (printpdf, whose azul-core 0.0.5 does not build for iOS) is off on iOS, where `chart::engrave` gives an empty `Engraved::pdf` and a song keeps the PDF it had, so an iPhone or iPad app links setlists (architect#640)
