// Render every page of a PDF to PNG with PDFKit (macOS).
//
//   swift scripts/signal/pdf-to-png.swift <in.pdf> <out-prefix> [width]
//
// Writes <out-prefix>-1.png, -2.png, … at `width` pixels (default 1600),
// on white, and prints each path it wrote.
import AppKit
import PDFKit

let args = CommandLine.arguments
guard args.count >= 3, let doc = PDFDocument(url: URL(fileURLWithPath: args[1])) else {
    FileHandle.standardError.write("usage: pdf-to-png.swift <in.pdf> <out-prefix> [width]\n".data(using: .utf8)!)
    exit(2)
}
let width = args.count > 3 ? (Double(args[3]) ?? 1600) : 1600
for i in 0..<doc.pageCount {
    guard let page = doc.page(at: i) else { continue }
    let bounds = page.bounds(for: .cropBox)
    let scale = width / bounds.width
    let w = Int((bounds.width * scale).rounded())
    let h = Int((bounds.height * scale).rounded())
    guard let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0),
        let ctx = NSGraphicsContext(bitmapImageRep: rep)
    else { continue }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = ctx
    let cg = ctx.cgContext
    cg.setFillColor(NSColor.white.cgColor)
    cg.fill(CGRect(x: 0, y: 0, width: w, height: h))
    cg.scaleBy(x: scale, y: scale)
    cg.translateBy(x: -bounds.origin.x, y: -bounds.origin.y)
    page.draw(with: .cropBox, to: cg)
    NSGraphicsContext.restoreGraphicsState()
    let out = "\(args[2])-\(i + 1).png"
    if let data = rep.representation(using: .png, properties: [:]) {
        try? data.write(to: URL(fileURLWithPath: out))
        print(out)
    }
}
